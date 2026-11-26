use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) struct TempDir(PathBuf);

impl TempDir {
    pub(super) fn new(prefix: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("{prefix}-{}-{time}-{sequence}", std::process::id()));
        fs::create_dir(&path).unwrap_or_else(|err| panic!("create {}: {err}", path.display()));
        Self(path)
    }

    pub(super) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // Don't mask a test failure if cleanup fails during unwinding.
        let _ = fs::remove_dir_all(&self.0);
    }
}
