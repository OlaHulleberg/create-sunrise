//! Diagnostics go to standard error. Set `CREATE_DEBUG_LOG` to an unused plain name such as
//! `create.log` to also keep a file under the instance's `sunrise-mod-data/create/`. Existing
//! files are left alone. Each line includes elapsed time and the thread name.

use super::data_dir::{DataDir, Leaf};
use mod_api::{InstanceDirectory, RegistrationContext};
use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

static LOG: OnceLock<(Instant, Option<Mutex<File>>)> = OnceLock::new();

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
        let file = (|| {
            let name = std::env::var_os("CREATE_DEBUG_LOG")?;
            let instance = cx.instance_dir().and_then(InstanceDirectory::to_path_buf)?;
            match open_log(&instance, &name) {
                Ok(file) => Some(Mutex::new(file)),
                Err(e) => {
                    let _ = writeln!(io::stderr().lock(), "[create] Diagnostic log disabled: {e}");
                    None
                }
            }
        })();
        (Instant::now(), file)
    });
}

/// Writes one line. Use `create_log!`.
pub fn line(args: std::fmt::Arguments<'_>) {
    let elapsed = LOG
        .get()
        .map_or(0.0, |(start, _)| start.elapsed().as_secs_f64());
    let thread = std::thread::current();
    let name = thread.name().unwrap_or("?");
    let _ = writeln!(io::stderr().lock(), "[{elapsed:9.3}] [{name}] {args}");
    if let Some((_, Some(file))) = LOG.get()
        && let Ok(mut file) = file.lock()
    {
        let _ = writeln!(file, "[{elapsed:9.3}] [{name}] {args}");
    }
}

/// `create_log!("[create] {n} states")`: a diagnostic line, with an optional copy in a file.
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
