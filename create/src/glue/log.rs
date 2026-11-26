//! Diagnostics are off by default. Set `CREATE_DEBUG_LOG` to an unused plain UTF-8 filename
//! such as `create.log` to write only to a new file under the instance's `sunrise-mod-data/create/`.
//! If the file cannot be created, nothing is logged. Each line includes time since the log opened
//! and the thread name.

use super::data_dir::{DataDir, Leaf};
use mod_api::{InstanceDirectory, RegistrationContext};
use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

static LOG: OnceLock<Option<(Mutex<File>, Instant)>> = OnceLock::new();

fn open_log(instance: &Path, name: &OsStr) -> io::Result<File> {
    let name = name
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "not a plain UTF-8 log name"))?;
    let leaf = Leaf::new(name)?;
    DataDir::open(instance, true)?.new_log(leaf)
}

/// The registration snapshot supplies the same instance path as the running host, losslessly.
pub(crate) fn register(cx: &RegistrationContext) {
    LOG.get_or_init(|| {
        let name = std::env::var_os("CREATE_DEBUG_LOG")?;
        let instance = cx.instance_dir().and_then(InstanceDirectory::to_path_buf)?;
        let file = open_log(&instance, &name).ok()?;
        Some((Mutex::new(file), Instant::now()))
    });
}

/// Writes one line if the log is enabled. Use `create_log!`.
pub fn line(args: std::fmt::Arguments<'_>) {
    if let Some(Some((file, start))) = LOG.get()
        && let Ok(mut file) = file.lock()
    {
        let elapsed = start.elapsed().as_secs_f64();
        let thread = std::thread::current();
        let name = thread.name().unwrap_or("?");
        let _ = writeln!(file, "[{elapsed:9.3}] [{name}] {args}");
    }
}

/// `create_log!("[create] {n} states")`: a line in the mod's log, when it has one.
#[macro_export]
macro_rules! create_log {
    ($($arg:tt)*) => {
        $crate::glue::log::line(format_args!($($arg)*))
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::glue::temp_dir::TempDir;
    use std::fs;

    #[test]
    fn opt_in_uses_only_a_new_file_in_create_data() {
        let instance = TempDir::new("create-log-test");
        assert!(open_log(instance.path(), OsStr::new("../other.log")).is_err());
        let data = instance.path().join("sunrise-mod-data");
        assert!(!data.exists());
        let mut sink = open_log(instance.path(), OsStr::new("debug.log")).unwrap();
        sink.write_all(b"line\n").unwrap();
        drop(sink);
        assert!(open_log(instance.path(), OsStr::new("debug.log")).is_err());
        assert_eq!(fs::read(data.join("create/debug.log")).unwrap(), b"line\n");
    }

    #[cfg(unix)]
    #[test]
    fn a_link_cannot_become_the_sink() {
        use std::os::unix::fs::symlink;
        let instance = TempDir::new("create-log-test");
        let outside = instance.path().join("unrelated.log");
        fs::write(&outside, b"keep").unwrap();
        drop(DataDir::open(instance.path(), true).unwrap());
        let data = instance.path().join("sunrise-mod-data/create");
        symlink(&outside, data.join("soft.log")).unwrap();
        fs::hard_link(&outside, data.join("hard.log")).unwrap();
        for link in ["soft.log", "hard.log"] {
            assert!(open_log(instance.path(), OsStr::new(link)).is_err());
        }
        assert_eq!(fs::read(outside).unwrap(), b"keep");
    }
}
