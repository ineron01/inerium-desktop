use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileCategory {
    Image,
    Video,
    Audio,
    Pdf,
    Document,
    Unknown,
}

/// Fast category detection by file extension without blocking I/O or magic byte sniffing.
pub fn detect_category(path: &Path) -> FileCategory {
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        let ext_lower = ext.to_lowercase();
        match ext_lower.as_str() {
            // Images
            "png" | "jpg" | "jpeg" | "jpe" | "webp" | "avif" | "jxl" | "gif" | "bmp"
            | "tiff" | "tif" | "ico" | "svg" | "heic" | "heif" | "raw" | "cr2" | "nef"
            | "arw" | "dng" | "psd" | "xcf" => return FileCategory::Image,

            // Video
            "mp4" | "m4v" | "mkv" | "webm" | "avi" | "mov" | "wmv" | "flv" | "ts" | "3gp"
            | "ogv" | "vob" => return FileCategory::Video,

            // Audio
            "mp3" | "flac" | "wav" | "ogg" | "opus" | "m4a" | "aac" | "wma" | "alac"
            | "aiff" | "ape" | "mid" | "midi" => return FileCategory::Audio,

            // PDF
            "pdf" => return FileCategory::Pdf,

            // Documents
            "docx" | "doc" | "odt" | "rtf" | "pptx" | "ppt" | "odp" | "xlsx" | "xls"
            | "ods" | "csv" | "txt" | "md" | "markdown" | "html" | "htm" | "epub" | "typ" => {
                return FileCategory::Document
            }

            _ => {}
        }
    }

    FileCategory::Unknown
}
