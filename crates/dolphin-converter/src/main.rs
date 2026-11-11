mod bg_remove;
mod category;
mod collision;
mod converter;
mod notify;

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use rayon::prelude::*;

use crate::category::{detect_category, FileCategory};
use crate::converter::convert_file;
use crate::notify::{notify_error, notify_info};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn print_help() {
    println!(
        r#"dolphin-convert v{VERSION} — Универсальный быстрый конвертер файлов для Dolphin (KDE Frameworks 6)

ИСПОЛЬЗОВАНИЕ:
    dolphin-convert --to <ФОРМАТ> [ПАРАМЕТРЫ] <ФАЙЛЫ...>

ПАРАМЕТРЫ:
    -f, -t, --format, --to <ФОРМАТ>   Целевой формат (png, png-nobg, webp, avif, jxl, mp4, mp3, pdf, docx и др.)
    -q, --quiet                       Отключить графические уведомления на рабочем столе
    -h, --help                        Показать данную справку
    -v, --version                     Показать версию утилиты

ПРИМЕРЫ:
    dolphin-convert --to png-nobg photo.jpg
    dolphin-convert --format webp photo1.png photo2.jpg
    dolphin-convert --format mp3 video.mp4
    dolphin-convert --format docx document.pdf
"#
    );
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() {
        print_help();
        return ExitCode::FAILURE;
    }

    let mut target_format: Option<String> = None;
    let mut quiet = false;
    let mut files: Vec<PathBuf> = Vec::new();

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                return ExitCode::SUCCESS;
            }
            "-v" | "--version" => {
                println!("dolphin-convert v{VERSION}");
                return ExitCode::SUCCESS;
            }
            "-q" | "--quiet" => {
                quiet = true;
            }
            "-f" | "-t" | "--format" | "--to" => {
                if let Some(val) = iter.next() {
                    target_format = Some(val.trim().trim_start_matches('.').to_lowercase());
                } else {
                    eprintln!("Ошибка: параметр --format/--to требует аргумент");
                    return ExitCode::FAILURE;
                }
            }
            other => {
                if other.starts_with("--format=") {
                    let val = other.trim_start_matches("--format=");
                    target_format = Some(val.trim().trim_start_matches('.').to_lowercase());
                } else if other.starts_with("--to=") {
                    let val = other.trim_start_matches("--to=");
                    target_format = Some(val.trim().trim_start_matches('.').to_lowercase());
                } else if other.starts_with('-') {
                    eprintln!("Неизвестный параметр: {}", other);
                    return ExitCode::FAILURE;
                } else {
                    files.push(PathBuf::from(other));
                }
            }
        }
    }

    let final_format = match target_format {
        Some(fmt) if !fmt.is_empty() => fmt,
        _ => {
            eprintln!("Ошибка: необходимо указать целевой формат через --to или --format <формат>");
            return ExitCode::FAILURE;
        }
    };

    if files.is_empty() {
        eprintln!("Ошибка: не указаны файлы для конвертации");
        return ExitCode::FAILURE;
    }

    // Filter existing files
    let existing_files: Vec<PathBuf> = files
        .into_iter()
        .filter(|p| {
            if p.exists() {
                true
            } else {
                eprintln!("Предупреждение: файл не найден: {:?}", p);
                false
            }
        })
        .collect();

    if existing_files.is_empty() {
        eprintln!("Ошибка: ни один из указанных файлов не существует");
        return ExitCode::FAILURE;
    }

    let total = existing_files.len();
    let format_upper = if final_format == "png-nobg" || final_format == "nobg" {
        "PNG (без фона AI)".to_string()
    } else {
        final_format.to_uppercase()
    };

    // Initial notification for batch tasks
    if !quiet && total > 1 {
        notify_info(
            "Конвертация файлов",
            &format!(
                "Начата конвертация {} файлов в формат {}...",
                total, format_upper
            ),
        );
    }

    // Heavy batch tasks (neural background removal or heavy video encoders) are processed sequentially
    // to preserve 100% CPU and avoid thread pool starvation
    let is_heavy_batch = final_format == "png-nobg" || final_format == "nobg";

    let (heavy_files, other_files): (Vec<PathBuf>, Vec<PathBuf>) = existing_files
        .into_iter()
        .partition(|f| is_heavy_batch || detect_category(f) == FileCategory::Video);

    let mut results: Vec<Result<Vec<PathBuf>, (PathBuf, String)>> = Vec::with_capacity(total);

    // 1. Process regular files in parallel via Rayon
    if !other_files.is_empty() {
        let parallel_results: Vec<Result<Vec<PathBuf>, (PathBuf, String)>> = other_files
            .par_iter()
            .map(|file_path| match convert_file(file_path, &final_format) {
                Ok(created_paths) => Ok(created_paths),
                Err(err) => Err((file_path.clone(), err)),
            })
            .collect();
        results.extend(parallel_results);
    }

    // 2. Process heavy tasks sequentially (AI background removal or video ffmpeg)
    for file_path in &heavy_files {
        match convert_file(file_path, &final_format) {
            Ok(created_paths) => results.push(Ok(created_paths)),
            Err(err) => results.push(Err((file_path.clone(), err))),
        }
    }

    let mut succeeded = 0;
    let mut failed = 0;
    let mut created_all: Vec<PathBuf> = Vec::new();
    let mut errors: Vec<(PathBuf, String)> = Vec::new();

    for res in results {
        match res {
            Ok(paths) => {
                succeeded += 1;
                created_all.extend(paths);
            }
            Err((path, err)) => {
                failed += 1;
                eprintln!("Ошибка конвертации {:?}: {}", path, err);
                errors.push((path, err));
            }
        }
    }

    // Final notifications
    if !quiet {
        if failed == 0 {
            if succeeded == 1 && !created_all.is_empty() {
                let name = created_all[0]
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("файл");
                notify_info(
                    "Конвертация завершена",
                    &format!("Готово: {} ({})", name, format_upper),
                );
            } else {
                notify_info(
                    "Конвертация завершена",
                    &format!(
                        "Успешно обработано {} файлов в формат {}",
                        succeeded, format_upper
                    ),
                );
            }
        } else if succeeded == 0 {
            let sample_err = &errors[0].1;
            notify_error(
                "Ошибка конвертации",
                &format!("Не удалось сконвертировать файлы: {}", sample_err),
            );
        } else {
            notify_info(
                "Конвертация частично завершена",
                &format!(
                    "Успешно: {}, с ошибками: {} (Формат {})",
                    succeeded, failed, format_upper
                ),
            );
        }
    }

    if failed > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
