//! What schematics read and write of the game beyond the hooks: the `schematics/` files (gzip
//! NBT, as `NbtIo` reads and writes them, through the engine's `gzip` and the API's NBT
//! encoding), a schematic's data on its stack (`minecraft:custom_data`) and each block's
//! item (`Block.asItem`).
//!
//! The files live in the mod's game data folder (`sunrise-mod-data/create/schematics/`): the
//! API reaches a mod's own folders only, not the game folder's `schematics/` where Create keeps
//! them.

use crate::glue::data_dir::{DataDir, Leaf};
use crate::glue::names::Named as _;
use crate::glue::packages::body::{CustomData, set_custom};
use create_engine::gzip;
use create_engine::schematic::{
    self as sch, Nbt, SchematicData, SchematicUpload, Structure, UploadRetention,
};
use mod_api::{Host, ItemStack, Tag};
use std::collections::HashMap;
use std::io::{self, Read};
use std::path::Path;

/// `SchematicItem.loadSchematic`'s `NbtAccounter.create(0x20000000L)`.
const MAX_BYTES: usize = 0x2000_0000;

const UPLOAD_BYTES: u64 = sch::MAX_TOTAL_SCHEMATIC_SIZE_KB * 1000;

fn folder(instance: &Path, create: bool) -> io::Result<DataDir> {
    DataDir::open(instance, create)?.subdir(Leaf::new("schematics")?, create)
}

fn file_leaf(file: &str) -> io::Result<Leaf<'_>> {
    if !sch::valid_file(file) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not an .nbt schematic name",
        ));
    }
    Leaf::new(file)
}

#[derive(Clone, Copy)]
pub(super) struct Uploaded<'a> {
    owner: Leaf<'a>,
    file: Leaf<'a>,
}

impl<'a> Uploaded<'a> {
    pub(super) fn new(owner: &'a str, file: &'a str) -> io::Result<Self> {
        if owner.starts_with('.') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "not a schematic owner",
            ));
        }
        Ok(Self {
            owner: Leaf::new(owner)?,
            file: file_leaf(file)?,
        })
    }

    fn folder(self, instance: &Path, create: bool) -> io::Result<DataDir> {
        folder(instance, create)?
            .subdir(Leaf::new("uploaded")?, create)?
            .subdir(self.owner, create)
    }
}

pub(crate) fn to_engine(t: &Tag) -> Nbt {
    match t {
        Tag::End => Nbt::Compound(Default::default()),
        Tag::Byte(v) => Nbt::Byte(*v),
        Tag::Short(v) => Nbt::Short(*v),
        Tag::Int(v) => Nbt::Int(*v),
        Tag::Long(v) => Nbt::Long(*v),
        Tag::Float(v) => Nbt::Float(*v),
        Tag::Double(v) => Nbt::Double(*v),
        Tag::ByteArray(v) => Nbt::ByteArray(v.clone()),
        Tag::String(s) => Nbt::String(s.to_string_lossy()),
        Tag::List(l) => Nbt::List(l.iter().map(to_engine).collect()),
        Tag::Compound(c) => Nbt::Compound(
            c.iter()
                .map(|(k, v)| (k.to_string_lossy(), to_engine(v)))
                .collect(),
        ),
        Tag::IntArray(v) => Nbt::IntArray(v.clone()),
        Tag::LongArray(v) => Nbt::LongArray(v.clone()),
    }
}

pub(crate) fn to_game(t: &Nbt) -> Tag {
    match t {
        Nbt::Byte(v) => Tag::Byte(*v),
        Nbt::Short(v) => Tag::Short(*v),
        Nbt::Int(v) => Tag::Int(*v),
        Nbt::Long(v) => Tag::Long(*v),
        Nbt::Float(v) => Tag::Float(*v),
        Nbt::Double(v) => Tag::Double(*v),
        Nbt::ByteArray(v) => Tag::ByteArray(v.clone()),
        Nbt::String(s) => Tag::String(s.as_str().into()),
        Nbt::List(l) => Tag::List(l.iter().map(to_game).collect()),
        Nbt::Compound(c) => Tag::Compound(
            c.iter()
                .map(|(k, v)| (k.as_str().into(), to_game(v)))
                .collect(),
        ),
        Nbt::IntArray(v) => Tag::IntArray(v.clone()),
        Nbt::LongArray(v) => Tag::LongArray(v.clone()),
    }
}

/// A file's root tag (`NbtIo.read`: a named root, its name dropped).
fn read_named(raw: &[u8]) -> Option<Tag> {
    // `[type][name length][name][payload]`; the API reads the nameless `[type][payload]`.
    let (&kind, rest) = raw.split_first()?;
    let (len, rest) = rest.split_first_chunk::<2>()?;
    let rest = rest.get(usize::from(u16::from_be_bytes(*len))..)?;
    let mut nameless = Vec::with_capacity(rest.len() + 1);
    nameless.push(kind);
    nameless.extend_from_slice(rest);
    Tag::from_network_bytes(&nameless, Some(MAX_BYTES as u64))
        .ok()
        .map(|(t, _)| t)
}

/// `tag` as a file's root, named "" (`NbtIo.write`).
fn write_named(tag: &Tag) -> Option<Vec<u8>> {
    let nameless = tag.to_network_bytes().ok()?;
    let (&kind, payload) = nameless.split_first()?;
    let mut named = Vec::with_capacity(nameless.len() + 2);
    named.extend_from_slice(&[kind, 0, 0]);
    named.extend_from_slice(payload);
    Some(named)
}

pub(super) fn decode_structure(file: &str, bytes: &[u8]) -> Option<Structure> {
    let raw = match gzip::decode(bytes, MAX_BYTES) {
        Ok(raw) => raw,
        Err(e) => {
            crate::create_log!("[create] Failed to read schematic {file}: {e:?}");
            return None;
        }
    };
    let structure = read_named(&raw).and_then(|tag| Structure::from_nbt(&to_engine(&tag)));
    if structure.is_none() {
        crate::create_log!("[create] Failed to read schematic {file}: invalid structure NBT");
    }
    structure
}

/// An independently supplied file in the root, with the NBT accounter but not the upload cap.
pub(super) fn read_local(host: &dyn Host, file: &str) -> Option<Structure> {
    let dir = folder(&host.instance_dir(), false).ok()?;
    let (mut handle, _) = dir.read_file(file_leaf(file).ok()?).ok()?;
    let mut bytes = Vec::new();
    handle.read_to_end(&mut bytes).ok()?;
    decode_structure(file, &bytes)
}

pub(super) fn read_uploaded(host: &dyn Host, target: Uploaded<'_>) -> Option<Structure> {
    let dir = target.folder(&host.instance_dir(), false).ok()?;
    let bytes = read_bounded(&dir, target.file).ok()?;
    decode_structure(target.file.name(), &bytes)
}

pub(super) fn encode_structure(structure: &Structure) -> Option<Vec<u8>> {
    let raw = write_named(&to_game(&structure.to_nbt()))?;
    Some(gzip::encode(&raw))
}

/// `SchematicExport`: choose a free filename and reserve it without opening an existing inode.
pub(super) fn write_export(host: &dyn Host, name: &str, bytes: &[u8]) -> io::Result<String> {
    let dir = folder(&host.instance_dir(), true)?;
    loop {
        let file = free_name(&dir, name)?;
        match dir.write_new(file_leaf(&file)?, bytes) {
            Ok(()) => return Ok(file),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(e),
        }
    }
}

fn free_name(dir: &DataDir, name: &str) -> io::Result<String> {
    let error = std::cell::OnceCell::new();
    let file = sch::export_name(name, false, |candidate| {
        match file_leaf(candidate).and_then(|leaf| dir.occupied(leaf)) {
            Ok(exists) => exists,
            Err(e) => {
                let _ = error.set(e);
                false
            }
        }
    });
    error.into_inner().map_or(Ok(file), Err)
}

/// The schematics saved in `schematics/` (`ClientSchematicLoader.refresh`), by name.
pub(super) fn local_files(host: &dyn Host) -> Vec<String> {
    let Ok(files) =
        folder(&host.instance_dir(), false).and_then(|d| d.regular_files(sch::valid_file))
    else {
        return Vec::new();
    };
    let mut names: Vec<_> = files.into_iter().map(|(name, _)| name).collect();
    names.sort();
    names
}

#[derive(Debug)]
pub(super) enum UploadReadError {
    Io(io::Error),
    TooLarge(u64),
}

impl From<io::Error> for UploadReadError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

fn read_bounded(dir: &DataDir, file: Leaf<'_>) -> Result<Vec<u8>, UploadReadError> {
    let (handle, meta) = dir.read_file(file)?;
    if !sch::size_allowed(meta.len()) {
        return Err(UploadReadError::TooLarge(meta.len()));
    }
    let mut bytes = Vec::new();
    // The file could grow after the fstat; its actual bytes are what count.
    handle.take(UPLOAD_BYTES + 1).read_to_end(&mut bytes)?;
    let size = bytes.len() as u64;
    if !sch::size_allowed(size) {
        return Err(UploadReadError::TooLarge(size));
    }
    Ok(bytes)
}

pub(super) fn read_for_upload(host: &dyn Host, file: &str) -> Result<Vec<u8>, UploadReadError> {
    read_bounded(&folder(&host.instance_dir(), false)?, file_leaf(file)?)
}

/// Publish already-validated upload bytes. Table replacement doesn't count the old destination;
/// instant replacement does, and replacing that same oldest file already retires it.
pub(super) fn store_upload(
    host: &dyn Host,
    target: Uploaded<'_>,
    upload: SchematicUpload,
    bytes: &[u8],
) -> io::Result<()> {
    store(
        &target.folder(&host.instance_dir(), true)?,
        target.file,
        upload,
        bytes,
    )
}

fn store(dir: &DataDir, file: Leaf<'_>, upload: SchematicUpload, bytes: &[u8]) -> io::Result<()> {
    if !sch::size_allowed(bytes.len() as u64) {
        return Err(io::Error::new(
            io::ErrorKind::FileTooLarge,
            "schematic exceeds upload limit",
        ));
    }
    let replaces = dir.existing_name(file)?;
    let mut retention = UploadRetention::default();
    for (name, meta) in dir.regular_files(sch::valid_file)? {
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_millis() as u64);
        retention.record(
            upload,
            replaces.as_deref().unwrap_or(file.name()),
            name,
            modified,
        );
    }
    dir.replace(file, bytes)?;
    if let Some(oldest) = retention
        .evict()
        .filter(|name| Some(name) != replaces.as_ref())
    {
        if let Err(e) = dir.remove(file_leaf(&oldest)?) {
            if replaces.is_none() {
                // A failed eviction must not increase the owner's count.
                dir.remove(file)?;
                return Err(e);
            }
            crate::create_log!("[create] Could not retire old schematic {oldest}: {e}");
        }
    }
    Ok(())
}

const KEYS: [&str; 7] = [
    "File", "Owner", "Deployed", "Anchor", "Rotation", "Mirror", "Bounds",
];

/// A stack's schematic data (empty for a stack without).
pub fn schematic_data(stack: &ItemStack) -> SchematicData {
    let Some(c) = stack.get(&CustomData) else {
        return SchematicData::default();
    };
    let found: HashMap<&str, Nbt> = KEYS
        .iter()
        .filter_map(|k| Some((*k, to_engine(c.get(&(*k).into())?))))
        .collect();
    SchematicData::from_entries(|k| found.get(k))
}

/// Writes `data` over the stack's schematic keys, keeping the rest of its custom data.
pub fn set_schematic_data(stack: &mut ItemStack, data: &SchematicData) {
    for k in KEYS {
        set_custom(stack, k, None);
    }
    for (k, v) in data.to_entries() {
        set_custom(stack, k, Some(to_game(&v)));
    }
}

/// `Block.asItem` (`Item.BY_BLOCK`): the item placing the block; a wall variant's
/// (`wall_torch`, `oak_wall_sign`) is its standing block's item.
pub fn item_for_block(host: &dyn Host, block: &str) -> Option<String> {
    let item = |name: &str| {
        let key = host.block_key(name)?;
        let item = host.block_item(key)?;
        let id = item.name(host);
        (id != "minecraft:air").then_some(id)
    };
    item(block).or_else(|| {
        let standing = block.replacen("_wall_", "_", 1).replacen(":wall_", ":", 1);
        item(&standing)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::glue::temp_dir::TempDir;

    #[test]
    fn upload_reads_and_replaces_a_regular_file() {
        let instance = TempDir::new("create-schematics");
        let target = Uploaded::new("Alex", "build.nbt").unwrap();
        let dir = target.folder(instance.path(), true).unwrap();
        dir.write_new(target.file, b"original").unwrap();
        assert_eq!(read_bounded(&dir, target.file).unwrap(), b"original");
        store(&dir, target.file, SchematicUpload::Table, b"updated").unwrap();
        assert_eq!(read_bounded(&dir, target.file).unwrap(), b"updated");
    }

    #[test]
    fn oversized_upload_does_not_replace_or_evict() {
        let instance = TempDir::new("create-schematic-cap");
        let target = Uploaded::new("Alex", "first.nbt").unwrap();
        let dir = target.folder(instance.path(), true).unwrap();
        for i in 0..sch::MAX_SCHEMATICS {
            dir.write_new(file_leaf(&format!("{i}.nbt")).unwrap(), b"keep")
                .unwrap();
        }
        let bytes = vec![0; UPLOAD_BYTES as usize + 1];
        assert!(store(&dir, target.file, SchematicUpload::Instant, &bytes).is_err());
        assert!(!dir.occupied(target.file).unwrap());
        assert_eq!(
            dir.regular_files(sch::valid_file).unwrap().len(),
            sch::MAX_SCHEMATICS
        );
        let source = folder(instance.path(), false).unwrap();
        source.write_new(target.file, &bytes).unwrap();
        assert!(matches!(
            read_bounded(&source, target.file),
            Err(UploadReadError::TooLarge(_))
        ));
    }

    #[test]
    fn files_keep_their_root_through_gzip() {
        let root = Tag::compound([
            ("DataVersion", Tag::Int(3955)),
            (
                "size",
                Tag::List(vec![Tag::Int(1), Tag::Int(2), Tag::Int(3)]),
            ),
            ("name", Tag::String("schematic".into())),
        ]);
        let raw = write_named(&root).expect("writable");
        // A named root: its type, an empty name, then the payload.
        assert_eq!(&raw[..3], &[10, 0, 0]);
        let back = gzip::decode(&gzip::encode(&raw), usize::MAX).expect("gzip");
        assert_eq!(read_named(&back), Some(root.clone()));
        let engine = to_engine(&root);
        assert_eq!(engine.get("DataVersion"), Some(&Nbt::Int(3955)));
        assert_eq!(to_game(&engine), root);
    }

    #[test]
    fn a_named_root_keeps_its_payload_past_its_name() {
        let root = Tag::compound([("a", Tag::Byte(1))]);
        let raw = write_named(&root).expect("writable");
        let mut named = vec![10, 0, 3, b'a', b'b', b'c'];
        named.extend_from_slice(&raw[3..]);
        assert_eq!(read_named(&named), Some(root));
    }
}
