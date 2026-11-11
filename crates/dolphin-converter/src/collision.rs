use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn next_counter() -> usize {
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Atomically reserves a file path in `parent_dir` using `O_CREAT | O_EXCL` (`create_new(true)`).
/// Prevents TOCTOU race conditions when multiple threads process files with matching stems concurrently
/// (e.g., `a.png` and `a.jpg` concurrently converted to `a.webp` and `a_1.webp`).
pub fn reserve_safe_output_path(parent_dir: &Path, stem: &str, ext: &str) -> Result<PathBuf, String> {
    let clean_ext = ext.trim_start_matches('.');
    let base_name = if clean_ext.is_empty() {
        stem.to_string()
    } else {
        format!("{}.{}", stem, clean_ext)
    };

    let first_choice = parent_dir.join(&base_name);
    if OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&first_choice)
        .is_ok()
    {
        return Ok(first_choice);
    }

    let mut index = 1;
    loop {
        let candidate_name = if clean_ext.is_empty() {
            format!("{}_{}", stem, index)
        } else {
            format!("{}_{}.{}", stem, index, clean_ext)
        };
        let candidate = parent_dir.join(&candidate_name);
        if OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
            .is_ok()
        {
            return Ok(candidate);
        }
        index += 1;
        if index > 100_000 {
            return Err("Превышен лимит попыток подбора уникального имени файла".to_string());
        }
    }
}

/// RAII Guard for an atomically reserved destination file.
/// If conversion fails or errors out before `commit()`, the reserved file is automatically removed on drop.
pub struct ReservedFile {
    pub path: PathBuf,
    committed: bool,
}

impl ReservedFile {
    pub fn new(parent_dir: &Path, stem: &str, ext: &str) -> Result<Self, String> {
        let path = reserve_safe_output_path(parent_dir, stem, ext)?;
        Ok(Self {
            path,
            committed: false,
        })
    }

    pub fn commit(mut self) {
        self.committed = true;
    }
}

impl Drop for ReservedFile {
    fn drop(&mut self) {
        if !self.committed && self.path.exists() {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// RAII Guard for a temporary directory in `std::env::temp_dir()` (/tmp).
/// Guarantees clean recursive removal on drop.
pub struct TempDirGuard {
    pub path: PathBuf,
}

impl TempDirGuard {
    pub fn new(prefix: &str) -> Result<Self, String> {
        let id = next_counter();
        let pid = std::process::id();
        let dir_name = format!("dolphin_convert_tmp_{}_{}_{}", pid, id, prefix);
        let path = std::env::temp_dir().join(dir_name);
        fs::create_dir_all(&path)
            .map_err(|e| format!("Не удалось создать временный каталог {:?}: {}", path, e))?;
        Ok(Self { path })
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

/// RAII Guard for an isolated LibreOffice user profile directory in `/tmp`.
/// Prevents lock contention and crashes when running multiple instances of `soffice`.
pub struct LibreOfficeProfileGuard {
    pub path: PathBuf,
}

impl LibreOfficeProfileGuard {
    pub fn new() -> Result<Self, String> {
        let id = next_counter();
        let pid = std::process::id();
        let dir_name = format!("soffice_profile_{}_{}", pid, id);
        let path = std::env::temp_dir().join(dir_name);
        fs::create_dir_all(&path)
            .map_err(|e| format!("Не удалось создать профиль LibreOffice {:?}: {}", path, e))?;
        Ok(Self { path })
    }

    pub fn env_arg(&self) -> String {
        format!("-env:UserInstallation=file://{}", self.path.display())
    }
}

impl Drop for LibreOfficeProfileGuard {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
