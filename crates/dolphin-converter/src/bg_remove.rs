use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use image::{imageops, GenericImageView, GrayImage, Luma};
use ort::session::Session;
use ort::value::Tensor;

use crate::collision::ReservedFile;

static SESSION: OnceLock<Mutex<Session>> = OnceLock::new();

/// Resolves the path to the silueta.onnx neural network model.
fn get_model_path() -> Result<PathBuf, String> {
    if let Ok(custom) = std::env::var("DOLPHIN_CONVERT_MODEL") {
        let p = PathBuf::from(custom);
        if p.exists() {
            return Ok(p);
        }
    }

    if let Ok(xdg_data) = std::env::var("XDG_DATA_HOME") {
        let candidate = PathBuf::from(xdg_data)
            .join("dolphin-converter/models/silueta.onnx");
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    if let Ok(home) = std::env::var("HOME") {
        let candidate = PathBuf::from(home)
            .join(".local/share/dolphin-converter/models/silueta.onnx");
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    Err("Модель сегментации silueta.onnx не найдена в ~/.local/share/dolphin-converter/models/ (укажите DOLPHIN_CONVERT_MODEL или скачайте её через install.sh).".to_string())
}

/// Initializes and returns the global ONNX Runtime session lazily.
/// Only triggered when neural background removal is explicitly requested.
fn get_session() -> Result<&'static Mutex<Session>, String> {
    if let Some(session) = SESSION.get() {
        return Ok(session);
    }

    let model_path = get_model_path()?;
    let lib_path = if Path::new("/usr/lib/libonnxruntime.so").exists() {
        "/usr/lib/libonnxruntime.so"
    } else {
        "libonnxruntime.so"
    };

    let env = ort::init_from(lib_path)
        .map_err(|e| format!("Не удалось инициализировать ONNX Runtime из {}: {}", lib_path, e))?
        .build()
        .map_err(|e| format!("Ошибка создания окружения ONNX Runtime: {}", e))?;

    let session = Session::builder(&env)
        .map_err(|e| format!("Ошибка Session::builder: {}", e))?
        .with_intra_threads(4)
        .map_err(|e| format!("Ошибка with_intra_threads: {}", e))?
        .commit_from_file(&model_path)
        .map_err(|e| format!("Не удалось загрузить модель {:?}: {}", model_path, e))?;

    let _ = SESSION.set(Mutex::new(session));
    Ok(SESSION.get().unwrap())
}

/// Removes background from the input image using Silueta ONNX segmentation model.
/// Saves the result into a transparent RGBA PNG with safe atomic file reservation.
pub fn remove_background(
    input_path: &Path,
    parent_dir: &Path,
    stem: &str,
) -> Result<Vec<PathBuf>, String> {
    let output_stem = format!("{}_nobg", stem);
    let reserved = ReservedFile::new(parent_dir, &output_stem, "png")?;

    let img = image::open(input_path)
        .map_err(|e| format!("Не удалось открыть изображение {:?}: {}", input_path, e))?;

    let (orig_width, orig_height) = img.dimensions();
    let mut rgba_img = img.to_rgba8();

    // 1. Resize copy to 320x320 model input
    let resized = imageops::resize(&rgba_img, 320, 320, imageops::FilterType::Triangle);

    // 2. Preprocess: ImageNet mean/std normalization and NCHW layout
    let mut input_data = vec![0.0f32; 3 * 320 * 320];
    let channel_stride = 320 * 320;
    let mean = [0.485f32, 0.456f32, 0.406f32];
    let std = [0.229f32, 0.224f32, 0.225f32];

    for y in 0..320 {
        for x in 0..320 {
            let p = resized.get_pixel(x, y);
            let idx = (y * 320 + x) as usize;
            input_data[idx] = ((p[0] as f32 / 255.0) - mean[0]) / std[0];
            input_data[channel_stride + idx] = ((p[1] as f32 / 255.0) - mean[1]) / std[1];
            input_data[2 * channel_stride + idx] = ((p[2] as f32 / 255.0) - mean[2]) / std[2];
        }
    }

    // 3. Inference
    let tensor = Tensor::from_array(([1, 3, 320, 320], input_data))
        .map_err(|e| format!("Ошибка создания входного тензора ONNX: {}", e))?;

    let session_mutex = get_session()?;
    let mut session = session_mutex
        .lock()
        .map_err(|e| format!("Ошибка блокировки сессии ONNX: {}", e))?;

    let outputs = session
        .run(ort::inputs![tensor])
        .map_err(|e| format!("Ошибка инференса модели: {}", e))?;

    let (_shape, mask_slice) = outputs[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| format!("Ошибка извлечения тензора маски: {}", e))?;

    // 4. Min-max normalization of output mask
    let mut min_val = f32::INFINITY;
    let mut max_val = f32::NEG_INFINITY;
    for &v in mask_slice {
        if v < min_val {
            min_val = v;
        }
        if v > max_val {
            max_val = v;
        }
    }
    let range = if (max_val - min_val).abs() > 1e-6 {
        max_val - min_val
    } else {
        1.0
    };

    let mut mask_320 = GrayImage::new(320, 320);
    for y in 0..320 {
        for x in 0..320 {
            let v = mask_slice[(y * 320 + x) as usize];
            let norm = ((v - min_val) / range * 255.0).clamp(0.0, 255.0) as u8;
            mask_320.put_pixel(x, y, Luma([norm]));
        }
    }

    // 5. Upscale mask back to original dimensions
    let mask_full = imageops::resize(
        &mask_320,
        orig_width,
        orig_height,
        imageops::FilterType::Triangle,
    );

    // 6. Apply mask into alpha channel
    for y in 0..orig_height {
        for x in 0..orig_width {
            let alpha = mask_full.get_pixel(x, y)[0];
            let p = rgba_img.get_pixel_mut(x, y);
            p[3] = ((p[3] as u16 * alpha as u16) / 255) as u8;
        }
    }

    // 7. Save output PNG
    rgba_img
        .save(&reserved.path)
        .map_err(|e| format!("Не удалось сохранить PNG файл {:?}: {}", reserved.path, e))?;

    let final_path = reserved.path.clone();
    reserved.commit();
    Ok(vec![final_path])
}
