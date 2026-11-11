use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::category::{detect_category, FileCategory};
use crate::collision::{LibreOfficeProfileGuard, ReservedFile, TempDirGuard};

/// Converts a single file into the requested target format.
/// Returns the list of created files (may be multiple in case of PDF page rasterization).
pub fn convert_file(input_path: &Path, target_fmt: &str) -> Result<Vec<PathBuf>, String> {
    if !input_path.exists() {
        return Err(format!("Файл не существует: {:?}", input_path));
    }

    let parent_dir = input_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();

    let stem = input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");

    let fmt = target_fmt.trim_start_matches('.').to_lowercase();
    let category = detect_category(input_path);

    if fmt == "png-nobg" || fmt == "nobg" {
        if category != FileCategory::Image && category != FileCategory::Unknown {
            return Err("Удаление фона поддерживается только для изображений".to_string());
        }
        return crate::bg_remove::remove_background(input_path, &parent_dir, stem);
    }

    match category {
        FileCategory::Image => convert_image(input_path, &parent_dir, stem, &fmt),
        FileCategory::Video => convert_video(input_path, &parent_dir, stem, &fmt),
        FileCategory::Audio => convert_audio(input_path, &parent_dir, stem, &fmt),
        FileCategory::Pdf => convert_pdf(input_path, &parent_dir, stem, &fmt),
        FileCategory::Document => convert_document(input_path, &parent_dir, stem, &fmt),
        FileCategory::Unknown => {
            // Try image first, fallback to video or pandoc/soffice
            if let Ok(res) = convert_image(input_path, &parent_dir, stem, &fmt) {
                return Ok(res);
            }
            if let Ok(res) = convert_video(input_path, &parent_dir, stem, &fmt) {
                return Ok(res);
            }
            convert_document(input_path, &parent_dir, stem, &fmt)
        }
    }
}

/// Converts images using ImageMagick (`magick`)
fn convert_image(
    input: &Path,
    parent: &Path,
    stem: &str,
    target_fmt: &str,
) -> Result<Vec<PathBuf>, String> {
    let reserved = ReservedFile::new(parent, stem, target_fmt)?;

    let is_svg = input
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("svg"))
        .unwrap_or(false);

    let mut cmd = Command::new("magick");

    if is_svg {
        cmd.args(["-density", "300"]);
    }

    cmd.arg(input);
    cmd.arg("-auto-orient");

    match target_fmt {
        "jpg" | "jpeg" => {
            cmd.args(["-background", "white", "-flatten", "-quality", "92"]);
        }
        "webp" => {
            cmd.args(["-quality", "85"]);
        }
        "avif" => {
            cmd.args(["-quality", "75"]);
        }
        "jxl" => {
            cmd.args(["-quality", "85"]);
        }
        "ico" => {
            cmd.args([
                "-background",
                "transparent",
                "-define",
                "icon:auto-resize=256,128,64,48,32,16",
            ]);
        }
        "tiff" | "tif" => {
            cmd.args(["-compress", "lzw"]);
        }
        _ => {}
    }

    cmd.arg(&reserved.path);

    let output = cmd
        .output()
        .map_err(|e| format!("Не удалось вызвать ImageMagick (magick): {}", e))?;

    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Ошибка ImageMagick: {}", err_msg.trim()));
    }

    let final_path = reserved.path.clone();
    reserved.commit();
    Ok(vec![final_path])
}

/// Converts video or extracts audio using FFmpeg
fn convert_video(
    input: &Path,
    parent: &Path,
    stem: &str,
    target_fmt: &str,
) -> Result<Vec<PathBuf>, String> {
    let reserved = ReservedFile::new(parent, stem, target_fmt)?;

    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(input);

    match target_fmt {
        // High quality animated GIF with 2-pass palette
        "gif" => {
            cmd.args([
                "-vf",
                "fps=15,scale='min(480,iw)':-1:flags=lanczos,split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse",
            ]);
        }
        // Video targets
        "mp4" => {
            cmd.args([
                "-c:v",
                "libx264",
                "-preset",
                "fast",
                "-crf",
                "22",
                "-c:a",
                "aac",
                "-b:a",
                "192k",
                "-movflags",
                "+faststart",
            ]);
        }
        "webm" => {
            cmd.args([
                "-c:v",
                "libvpx-vp9",
                "-crf",
                "30",
                "-b:v",
                "0",
                "-c:a",
                "libopus",
                "-b:a",
                "128k",
            ]);
        }
        "mkv" => {
            cmd.args([
                "-c:v", "libx264", "-preset", "fast", "-crf", "22", "-c:a", "aac", "-b:a", "192k",
            ]);
        }
        "avi" => {
            cmd.args(["-c:v", "mpeg4", "-q:v", "3", "-c:a", "mp3"]);
        }
        "mov" => {
            cmd.args([
                "-c:v", "libx264", "-preset", "fast", "-crf", "22", "-c:a", "aac", "-b:a", "192k",
            ]);
        }
        // Audio extraction from video
        "mp3" => {
            cmd.args(["-vn", "-c:a", "libmp3lame", "-q:a", "2"]);
        }
        "wav" => {
            cmd.args(["-vn", "-c:a", "pcm_s16le"]);
        }
        "flac" => {
            cmd.args(["-vn", "-c:a", "flac"]);
        }
        "ogg" => {
            cmd.args(["-vn", "-c:a", "libvorbis", "-q:a", "6"]);
        }
        "opus" => {
            cmd.args(["-vn", "-c:a", "libopus", "-b:a", "160k"]);
        }
        "m4a" | "aac" => {
            cmd.args(["-vn", "-c:a", "aac", "-b:a", "256k"]);
        }
        _ => {}
    }

    cmd.arg(&reserved.path);

    let output = cmd
        .output()
        .map_err(|e| format!("Не удалось вызвать FFmpeg: {}", e))?;

    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Ошибка FFmpeg: {}", err_msg.trim()));
    }

    let final_path = reserved.path.clone();
    reserved.commit();
    Ok(vec![final_path])
}

/// Converts audio files using FFmpeg
fn convert_audio(
    input: &Path,
    parent: &Path,
    stem: &str,
    target_fmt: &str,
) -> Result<Vec<PathBuf>, String> {
    let reserved = ReservedFile::new(parent, stem, target_fmt)?;

    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(input);

    match target_fmt {
        "mp3" => {
            cmd.args(["-c:a", "libmp3lame", "-b:a", "320k"]);
        }
        "flac" => {
            cmd.args(["-c:a", "flac"]);
        }
        "wav" => {
            cmd.args(["-c:a", "pcm_s16le"]);
        }
        "ogg" => {
            cmd.args(["-c:a", "libvorbis", "-q:a", "6"]);
        }
        "opus" => {
            cmd.args(["-c:a", "libopus", "-b:a", "160k"]);
        }
        "m4a" | "aac" => {
            cmd.args(["-c:a", "aac", "-b:a", "256k"]);
        }
        "wma" => {
            cmd.args(["-c:a", "wmav2", "-b:a", "192k"]);
        }
        _ => {}
    }

    cmd.arg(&reserved.path);

    let output = cmd
        .output()
        .map_err(|e| format!("Не удалось вызвать FFmpeg: {}", e))?;

    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Ошибка кодека FFmpeg: {}", err_msg.trim()));
    }

    let final_path = reserved.path.clone();
    reserved.commit();
    Ok(vec![final_path])
}

/// Converts PDF documents (rasterization to images, extraction to text, or conversion to office docs)
fn convert_pdf(
    input: &Path,
    parent: &Path,
    stem: &str,
    target_fmt: &str,
) -> Result<Vec<PathBuf>, String> {
    match target_fmt {
        "png" | "jpg" | "jpeg" => {
            // Temporary rasterization directory strictly in /tmp
            let temp_dir_guard = TempDirGuard::new(stem)?;
            let prefix = temp_dir_guard.path.join("page");

            let mut cmd = Command::new("pdftoppm");
            if target_fmt == "png" {
                cmd.arg("-png");
            } else {
                cmd.arg("-jpeg");
            }

            cmd.args(["-r", "150"]).arg(input).arg(&prefix);

            let output = cmd
                .output()
                .map_err(|e| format!("Не удалось запустить pdftoppm: {}", e))?;

            if !output.status.success() {
                let err_msg = String::from_utf8_lossy(&output.stderr);
                return Err(format!("Ошибка pdftoppm: {}", err_msg.trim()));
            }

            // Find all generated page images
            let mut pages = Vec::new();
            if let Ok(entries) = fs::read_dir(&temp_dir_guard.path) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if let Some(ext) = path.extension().and_then(|s| s.to_str())
                        && (ext.eq_ignore_ascii_case(target_fmt)
                            || (target_fmt == "jpg" && ext.eq_ignore_ascii_case("jpeg")))
                    {
                        pages.push(path);
                    }
                }
            }

            pages.sort();

            if pages.is_empty() {
                return Err("pdftoppm не создал ни одной страницы".to_string());
            }

            let mut final_paths = Vec::new();
            if pages.len() == 1 {
                let reserved = ReservedFile::new(parent, stem, target_fmt)?;
                if let Err(_e) = fs::rename(&pages[0], &reserved.path) {
                    fs::copy(&pages[0], &reserved.path).map_err(|copy_err| {
                        format!("Не удалось скопировать страницу: {}", copy_err)
                    })?;
                }
                let final_dest = reserved.path.clone();
                reserved.commit();
                final_paths.push(final_dest);
            } else {
                for (idx, page_file) in pages.iter().enumerate() {
                    let page_stem = format!("{}_page_{}", stem, idx + 1);
                    let reserved = ReservedFile::new(parent, &page_stem, target_fmt)?;
                    if let Err(_e) = fs::rename(page_file, &reserved.path) {
                        fs::copy(page_file, &reserved.path).map_err(|copy_err| {
                            format!("Не удалось скопировать страницу: {}", copy_err)
                        })?;
                    }
                    let final_dest = reserved.path.clone();
                    reserved.commit();
                    final_paths.push(final_dest);
                }
            }

            Ok(final_paths)
        }

        "txt" => {
            let reserved = ReservedFile::new(parent, stem, "txt")?;

            let output = Command::new("pdftotext")
                .args(["-layout"])
                .arg(input)
                .arg(&reserved.path)
                .output()
                .map_err(|e| format!("Не удалось запустить pdftotext: {}", e))?;

            if !output.status.success() {
                let err_msg = String::from_utf8_lossy(&output.stderr);
                return Err(format!("Ошибка pdftotext: {}", err_msg.trim()));
            }

            let final_path = reserved.path.clone();
            reserved.commit();
            Ok(vec![final_path])
        }

        "docx" | "odt" | "html" => {
            // Crucial fix: LibreOffice defaults to Draw for PDF, which lacks docx/odt exporters.
            // We must pass --infilter=writer_pdf_import to force Writer import.
            convert_with_libreoffice(input, parent, stem, target_fmt, Some("writer_pdf_import"))
        }

        _ => Err(format!("Формат {} не поддерживается для PDF", target_fmt)),
    }
}

/// Converts documents using typst, pandoc, or LibreOffice
fn convert_document(
    input: &Path,
    parent: &Path,
    stem: &str,
    target_fmt: &str,
) -> Result<Vec<PathBuf>, String> {
    let input_ext = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    // 1. Typst files
    if input_ext == "typ" {
        let reserved = ReservedFile::new(parent, stem, target_fmt)?;

        let output = Command::new("typst")
            .arg("compile")
            .arg(input)
            .arg(&reserved.path)
            .output()
            .map_err(|e| format!("Не удалось запустить typst: {}", e))?;

        if !output.status.success() {
            let err_msg = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Ошибка Typst: {}", err_msg.trim()));
        }

        let final_path = reserved.path.clone();
        reserved.commit();
        return Ok(vec![final_path]);
    }

    // 2. Markdown or Plain Text via Pandoc
    if input_ext == "md" || input_ext == "markdown" || input_ext == "txt" {
        let reserved = ReservedFile::new(parent, stem, target_fmt)?;

        let mut cmd = Command::new("pandoc");
        cmd.arg(input);

        if target_fmt == "pdf" {
            cmd.arg("--pdf-engine=typst");
        } else if target_fmt == "html" {
            cmd.arg("-s");
        }

        cmd.arg("-o").arg(&reserved.path);

        if let Ok(output) = cmd.output()
            && output.status.success()
        {
            let final_path = reserved.path.clone();
            reserved.commit();
            return Ok(vec![final_path]);
        }
        // If pandoc fails on this format, reserved drops (removing candidate file), fallback to LibreOffice
    }

    // 3. Office Documents (docx, odt, rtf, pptx, xlsx, etc.) via LibreOffice
    convert_with_libreoffice(input, parent, stem, target_fmt, None)
}

/// Converts documents via LibreOffice headless mode with an isolated user profile and sandboxed temp dir in /tmp.
fn convert_with_libreoffice(
    input: &Path,
    parent: &Path,
    stem: &str,
    target_fmt: &str,
    infilter: Option<&str>,
) -> Result<Vec<PathBuf>, String> {
    // Unique profile in /tmp prevents profile lock conflicts during Rayon multi-file conversions.
    let profile_guard = LibreOfficeProfileGuard::new()?;

    // Temporary output directory strictly in /tmp
    let temp_dir_guard = TempDirGuard::new(stem)?;

    let filter_arg = match target_fmt {
        "txt" => "txt:Text",
        _ => target_fmt,
    };

    let mut cmd = Command::new("soffice");
    cmd.arg(profile_guard.env_arg());
    cmd.arg("--headless");

    if let Some(inf) = infilter {
        cmd.arg(format!("--infilter={}", inf));
    }

    cmd.args(["--convert-to", filter_arg]);
    cmd.arg(input);
    cmd.arg("--outdir");
    cmd.arg(&temp_dir_guard.path);

    let output = cmd
        .output()
        .map_err(|e| format!("Не удалось запустить LibreOffice (soffice): {}", e))?;

    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Ошибка LibreOffice: {}", err_msg.trim()));
    }

    // Locate generated file(s) in temp_dir
    let mut produced_files = Vec::new();
    if let Ok(entries) = fs::read_dir(&temp_dir_guard.path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                produced_files.push(path);
            }
        }
    }

    if produced_files.is_empty() {
        return Err(format!(
            "LibreOffice завершился, но не создал выходной файл формата {}",
            target_fmt
        ));
    }

    let mut final_paths = Vec::new();
    for produced in produced_files {
        let reserved = ReservedFile::new(parent, stem, target_fmt)?;
        if let Err(_e) = fs::rename(&produced, &reserved.path) {
            fs::copy(&produced, &reserved.path)
                .map_err(|copy_err| format!("Не удалось переместить результат: {}", copy_err))?;
        }
        let final_dest = reserved.path.clone();
        reserved.commit();
        final_paths.push(final_dest);
    }

    Ok(final_paths)
}
