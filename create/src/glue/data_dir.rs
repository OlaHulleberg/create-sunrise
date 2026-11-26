//! The Create data folder under the game's trusted instance directory. Each directory is
//! checked before use or creation, and existing leaves must be regular files, not links.
//! This does not defend against another process changing directories between checks and use.

use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy, Debug)]
pub(super) struct Leaf<'a>(&'a str);

impl<'a> Leaf<'a> {
    pub(super) fn new(name: &'a str) -> io::Result<Self> {
        // Apply the same filename rules on both ends of a connection. Windows recognizes
        // device names even with an extension, and ':' can select an alternate data stream.
        let stem = name
            .split('.')
            .next()
            .unwrap_or_default()
            .trim_end_matches(' ');
        let device = stem.to_ascii_uppercase();
        let reserved = matches!(
            device.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
        ) || device
            .strip_prefix("COM")
            .or_else(|| device.strip_prefix("LPT"))
            .is_some_and(|n| {
                matches!(
                    n,
                    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            });
        if name.is_empty()
            || name.ends_with(['.', ' '])
            || name.contains(['/', '\\', ':', '<', '>', '"', '|', '?', '*'])
            || name.bytes().any(|b| b < b' ')
            || reserved
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "not a plain, portable file name",
            ));
        }
        Ok(Self(name))
    }

    pub(super) fn name(self) -> &'a str {
        self.0
    }
}

/// Its path is the real path of the checked directory, not a client-supplied path.
pub(super) struct DataDir(PathBuf);

impl DataDir {
    pub(super) fn open(instance: &Path, create: bool) -> io::Result<Self> {
        Self(instance.canonicalize()?)
            .subdir(Leaf("sunrise-mod-data"), create)?
            .subdir(Leaf("create"), create)
    }

    pub(super) fn subdir(&self, leaf: Leaf<'_>, create: bool) -> io::Result<Self> {
        let path = self.0.join(leaf.0);
        let meta = match path.symlink_metadata() {
            Err(e) if create && e.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(&path).or_else(|e| {
                    if e.kind() == io::ErrorKind::AlreadyExists {
                        Ok(())
                    } else {
                        Err(e)
                    }
                })?;
                path.symlink_metadata()?
            }
            result => result?,
        };
        if is_link(&meta) || !meta.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "not a real Create data directory",
            ));
        }
        let real = path.canonicalize()?;
        if real.parent() != Some(self.0.as_path()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "directory outside Create data",
            ));
        }
        Ok(Self(real))
    }

    pub(super) fn read_file(&self, leaf: Leaf<'_>) -> io::Result<(File, Metadata)> {
        self.open_file(leaf, OpenOptions::new().read(true))
    }

    fn open_file(&self, leaf: Leaf<'_>, options: &OpenOptions) -> io::Result<(File, Metadata)> {
        let path = self.0.join(leaf.0);
        // Don't open links or special files (in particular, opening a FIFO can block).
        check_file(&path.symlink_metadata()?)?;
        let file = options.open(path)?;
        let metadata = file.metadata()?;
        check_file(&metadata)?;
        Ok((file, metadata))
    }

    pub(super) fn occupied(&self, leaf: Leaf<'_>) -> io::Result<bool> {
        // An unsafe leaf also occupies a name. Do not open it to choose the next suffix.
        match self.0.join(leaf.0).symlink_metadata() {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }

    pub(super) fn regular_files(
        &self,
        accept: impl Fn(&str) -> bool,
    ) -> io::Result<Vec<(String, Metadata)>> {
        let mut files = Vec::new();
        for entry in self.0.read_dir()? {
            let Ok(name) = entry?.file_name().into_string() else {
                continue;
            };
            if accept(&name)
                && let Ok(leaf) = Leaf::new(&name)
                && let Ok((_, meta)) = self.read_file(leaf)
            {
                files.push((name, meta));
            }
        }
        Ok(files)
    }

    pub(super) fn existing_name(&self, leaf: Leaf<'_>) -> io::Result<Option<String>> {
        let path = self.0.join(leaf.0);
        match path.symlink_metadata() {
            Ok(meta) => check_file(&meta)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        }
        // On a case-insensitive filesystem the spelling on disk may differ from the request.
        let real = path.canonicalize()?;
        let name = real
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|_| real.parent() == Some(self.0.as_path()))
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "file outside Create data")
            })?;
        Ok(Some(name.to_owned()))
    }

    pub(super) fn write_new(&self, leaf: Leaf<'_>, bytes: &[u8]) -> io::Result<()> {
        let path = self.0.join(leaf.0);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        let written = file
            .metadata()
            .and_then(|meta| check_file(&meta))
            .and_then(|()| file.write_all(bytes));
        drop(file);
        if written.is_err() {
            let _ = fs::remove_file(path);
        }
        written
    }

    pub(super) fn replace(&self, leaf: Leaf<'_>, bytes: &[u8]) -> io::Result<()> {
        // Re-check the destination without opening its inode for writing.
        let _ = self.existing_name(leaf)?;
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let staged = loop {
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let name = format!(".create-stage-{}-{n}", std::process::id());
            match self.write_new(Leaf(&name), bytes) {
                Ok(()) => break name,
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
                Err(e) => return Err(e),
            }
        };
        let staged = self.0.join(staged);
        let renamed = fs::rename(&staged, self.0.join(leaf.0));
        if renamed.is_err() {
            let _ = fs::remove_file(staged);
        }
        renamed
    }

    pub(super) fn remove(&self, leaf: Leaf<'_>) -> io::Result<()> {
        let path = self.0.join(leaf.0);
        check_file(&path.symlink_metadata()?)?;
        fs::remove_file(path)
    }
}

fn is_link(meta: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    meta.is_symlink()
}

fn check_file(meta: &Metadata) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        if meta.nlink() != 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "multiply linked Create data file",
            ));
        }
    }
    if is_link(meta) || !meta.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "not a regular Create data file",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_names_are_single_leaves() {
        for name in [
            "",
            ".",
            "..",
            "../a",
            "a/b",
            "a\\b",
            "a:",
            "a.nbt:ads",
            "file.",
            "file ",
            "NuL.nbt",
            "COM¹.nbt",
            "LPT2 .nbt",
            "CONOUT$",
            "a\0b",
        ] {
            assert!(Leaf::new(name).is_err(), "{name:?}");
        }
        for name in ["a.nbt", "hello world.nbt", "日.nbt"] {
            assert!(Leaf::new(name).is_ok(), "{name:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn linked_directories_and_leaves_are_not_followed() {
        use std::os::unix::fs::symlink;
        let instance = crate::glue::temp_dir::TempDir::new("create-instance");
        let other = crate::glue::temp_dir::TempDir::new("create-outside");
        let fixed = instance.path().join("sunrise-mod-data");
        symlink(other.path(), &fixed).unwrap();
        assert!(DataDir::open(instance.path(), true).is_err());
        assert!(!other.path().join("create").exists());
        fs::remove_file(&fixed).unwrap();

        let data = DataDir::open(instance.path(), true).unwrap();
        let root = fixed.join("create");
        symlink(other.path(), root.join("owner")).unwrap();
        assert!(data.subdir(Leaf("owner"), true).is_err());
        let outside = other.path().join("stay.nbt");
        fs::write(&outside, b"outside").unwrap();
        symlink(&outside, root.join("soft.nbt")).unwrap();
        fs::hard_link(&outside, root.join("hard.nbt")).unwrap();
        for name in [Leaf("soft.nbt"), Leaf("hard.nbt")] {
            assert!(data.read_file(name).is_err());
            assert!(data.replace(name, b"bad").is_err());
        }
        assert!(data.regular_files(|_| true).unwrap().is_empty());
        assert_eq!(fs::read(outside).unwrap(), b"outside");
    }
}
