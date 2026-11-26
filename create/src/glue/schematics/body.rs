//! What schematics read and write of the game beyond the hooks: the `schematics/` files (gzip
//! NBT, as `NbtIo` reads and writes them, through the engine's `gzip` and the API's NBT
//! encoding), a schematic's data on its stack (`minecraft:custom_data`) and each block's
//! item (`Block.asItem`).
//!
//! The files live in the mod's game data folder (`sunrise-mod-data/create/schematics/`): the
//! API reaches a mod's own folders only, not the game folder's `schematics/` where Create keeps
//! them.

use super::saved_data::Access;
use crate::glue::data_dir::{DataDir, Leaf};
use crate::glue::names::Named as _;
use crate::glue::packages::body::{CustomData, set_custom};
use create_engine::gzip;
use create_engine::schematic::{
    self as sch, Nbt, SchematicData, SchematicUpload, Structure, UploadRetention,
};
use mod_api::{ClientHost, Host, ItemStack, Tag};
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
        Ok(Self {
            owner: owner_leaf(owner)?,
            file: file_leaf(file)?,
        })
    }

    fn folder(self, instance: &Path, create: bool) -> io::Result<DataDir> {
        owner_folder(&folder(instance, create)?, self.owner, create)
    }
}

fn owner_leaf(owner: &str) -> io::Result<Leaf<'_>> {
    if owner.starts_with('.') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a schematic owner",
        ));
    }
    Leaf::new(owner)
}

fn owner_folder(root: &DataDir, owner: Leaf<'_>, create: bool) -> io::Result<DataDir> {
    root.subdir(Leaf::new("uploaded")?, create)?
        .subdir(owner, create)
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
fn read_local(host: &dyn Host, file: &str) -> Option<Structure> {
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
        let file = free_name(&[&dir], name)?;
        match dir.write_new(file_leaf(&file)?, bytes) {
            Ok(()) => return Ok(file),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(e),
        }
    }
}

fn free_name(dirs: &[&DataDir], name: &str) -> io::Result<String> {
    let error = std::cell::OnceCell::new();
    let file = sch::export_name(name, false, |candidate| {
        match file_leaf(candidate).and_then(|leaf| {
            for dir in dirs {
                if dir.occupied(leaf)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }) {
            Ok(exists) => exists,
            Err(e) => {
                let _ = error.set(e);
                false
            }
        }
    });
    error.into_inner().map_or(Ok(file), Err)
}

/// The existing table selection remains ordinal; both sides of an integrated game enumerate
/// the same regular root files and its owner's saved ones in the same order.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum TableFile {
    Local(String),
    Saved(String),
}

impl TableFile {
    pub(super) fn name(&self) -> &str {
        match self {
            Self::Local(name) | Self::Saved(name) => name,
        }
    }

    pub(super) fn label(&self) -> std::borrow::Cow<'_, str> {
        match self {
            Self::Local(name) => name.as_str().into(),
            Self::Saved(name) => format!("{name} [saved]").into(),
        }
    }
}

fn integrated_owner(host: &dyn ClientHost) -> Option<String> {
    host.client_session()
        .client_server()?
        .integrated_local
        .then_some(())?;
    host.local_player()?.entity().name(host)
}

pub(super) fn client_table_files(host: &dyn ClientHost) -> Vec<TableFile> {
    table_files(host, integrated_owner(host).as_deref())
}

pub(super) fn table_files(host: &dyn Host, owner: Option<&str>) -> Vec<TableFile> {
    list_files(&host.instance_dir(), owner)
}

fn list_files(instance: &Path, owner: Option<&str>) -> Vec<TableFile> {
    let Ok(root) = folder(instance, false) else {
        return Vec::new();
    };
    let mut out: Vec<_> = root
        .regular_files(sch::valid_file)
        .unwrap_or_default()
        .into_iter()
        .map(|(name, _)| TableFile::Local(name))
        .collect();
    if let Some(owner) = owner.and_then(|owner| owner_leaf(owner).ok())
        && let Ok(dir) = owner_folder(&root, owner, false)
    {
        out.extend(
            dir.regular_files(sch::valid_file)
                .unwrap_or_default()
                .into_iter()
                .map(|(name, _)| TableFile::Saved(name)),
        );
    }
    out.sort_by(|a, b| {
        a.name()
            .cmp(b.name())
            .then_with(|| matches!(a, TableFile::Saved(_)).cmp(&matches!(b, TableFile::Saved(_))))
    });
    out
}

/// Client-side lookup from an actual schematic item's identity. A transfer never grants the
/// holder access to the other owner's files on this client; root previews remain independent.
pub(super) fn preview_structure(
    host: &dyn ClientHost,
    owner: Option<&str>,
    file: &str,
) -> Option<Structure> {
    if let Some(owner) = owner
        && integrated_owner(host).as_deref() == Some(owner)
        && let Ok(target) = Uploaded::new(owner, file)
        && let Some(structure) = read_uploaded(host, target)
    {
        return Some(structure);
    }
    read_local(host, file)
}

#[derive(Debug)]
pub(super) enum UploadError {
    Io(io::Error),
    TooLarge(u64),
}

impl From<io::Error> for UploadError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

fn read_bounded(dir: &DataDir, file: Leaf<'_>) -> Result<Vec<u8>, UploadError> {
    let (handle, meta) = dir.read_file(file)?;
    if !sch::size_allowed(meta.len()) {
        return Err(UploadError::TooLarge(meta.len()));
    }
    let mut bytes = Vec::new();
    // The file could grow after the fstat; its actual bytes are what count.
    handle.take(UPLOAD_BYTES + 1).read_to_end(&mut bytes)?;
    let size = bytes.len() as u64;
    if !sch::size_allowed(size) {
        return Err(UploadError::TooLarge(size));
    }
    Ok(bytes)
}

pub(super) fn read_for_table(
    host: &dyn Host,
    target: Uploaded<'_>,
    source: &TableFile,
) -> Result<Vec<u8>, UploadError> {
    let instance = host.instance_dir();
    let dir = match source {
        TableFile::Local(_) => folder(&instance, false)?,
        TableFile::Saved(_) => target.folder(&instance, false)?,
    };
    read_bounded(&dir, target.file)
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
    UploadPlan::new(dir, file, upload)?.commit(bytes)
}

struct UploadPlan<'a> {
    dir: &'a DataDir,
    file: Leaf<'a>,
    replaces: bool,
    evict: Option<String>,
}

impl<'a> UploadPlan<'a> {
    fn new(dir: &'a DataDir, file: Leaf<'a>, upload: SchematicUpload) -> io::Result<Self> {
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
        let evict = retention
            .evict()
            .filter(|name| Some(name) != replaces.as_ref());
        Ok(Self {
            dir,
            file,
            replaces: replaces.is_some(),
            evict,
        })
    }

    fn commit(self, bytes: &[u8]) -> io::Result<()> {
        if self.replaces {
            self.dir.replace(self.file, bytes)?;
        } else {
            self.dir.write_new(self.file, bytes)?;
        }
        if let Some(oldest) = self.evict
            && let Err(e) = self.dir.remove(file_leaf(&oldest)?)
        {
            if !self.replaces {
                // A failed eviction must not increase the owner's count.
                self.dir.remove(self.file)?;
                return Err(e);
            }
            crate::create_log!("[create] Could not retire old schematic {oldest}: {e}");
        }
        Ok(())
    }
}

pub(super) struct QuillReceipt {
    pub slot: u32,
    pub file: String,
    pub size: [i32; 3],
}

/// Both quill actions use one per-owner destination. Nothing is captured before slot and
/// destination admission; nothing is written or retired before the actual gzip byte check.
pub(super) fn capture_to_upload(
    instance: &Path,
    owner: &str,
    name: &str,
    selected_quill: Option<u32>,
    access: Access,
    capture: impl FnOnce(Access) -> Option<Structure>,
) -> Result<Option<QuillReceipt>, UploadError> {
    let Some(slot) = selected_quill else {
        return Ok(None);
    };
    let owner = owner_leaf(owner)?;
    let root = folder(instance, true)?;
    let dir = owner_folder(&root, owner, true)?;
    // Preserve independent local files, including those with the same requested slug.
    let file = free_name(&[&dir, &root], name)?;
    let plan = UploadPlan::new(&dir, file_leaf(&file)?, SchematicUpload::Instant)?;
    let (size, bytes) = capture(access)
        .and_then(|structure| Some((structure.size, encode_structure(&structure)?)))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "schematic cannot be encoded"))?;
    let compressed_size = bytes.len() as u64;
    if !sch::size_allowed(compressed_size) {
        return Err(UploadError::TooLarge(compressed_size));
    }
    plan.commit(&bytes)?;
    Ok(Some(QuillReceipt { slot, file, size }))
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
    use std::collections::BTreeMap;

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
            Err(UploadError::TooLarge(_))
        ));
    }

    #[test]
    fn quill_refusal_does_not_capture_or_create_directories() {
        let instance = TempDir::new("create-quill-admission");
        assert!(
            capture_to_upload(
                instance.path(),
                "Alex",
                "test",
                None,
                Access::Ordinary,
                |_| { panic!("wrong held item must not capture") }
            )
            .unwrap()
            .is_none()
        );
        assert!(
            capture_to_upload(
                instance.path(),
                "../outside",
                "test",
                Some(1),
                Access::Ordinary,
                |_| { panic!("invalid owner must not capture") }
            )
            .is_err()
        );
        assert!(!instance.path().join("sunrise-mod-data").exists());
    }

    #[test]
    fn quill_checks_actual_gzip_then_retires_at_capacity() {
        let instance = TempDir::new("create-quill-retention");
        let root = folder(instance.path(), true).unwrap();
        let dir = owner_folder(&root, owner_leaf("Alex").unwrap(), true).unwrap();
        root.write_new(file_leaf("build.nbt").unwrap(), b"independent")
            .unwrap();
        for i in 0..sch::MAX_SCHEMATICS {
            dir.write_new(file_leaf(&format!("old_{i}.nbt")).unwrap(), b"existing")
                .unwrap();
        }
        let oldest = file_leaf("old_0.nbt").unwrap();
        let old_path = instance
            .path()
            .join("sunrise-mod-data/create/schematics/uploaded/Alex/old_0.nbt");
        std::fs::OpenOptions::new()
            .write(true)
            .open(old_path)
            .unwrap()
            .set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(3600))
            .unwrap();
        let fixture = Structure {
            size: [1, 1, 1],
            entities: vec![sch::EntityInfo {
                pos: [0.5; 3],
                block_pos: [0; 3],
                nbt: Nbt::Compound(BTreeMap::from([
                    ("id".into(), Nbt::String("minecraft:armor_stand".into())),
                    (
                        "size_fixture".into(),
                        Nbt::ByteArray(vec![0; UPLOAD_BYTES as usize]),
                    ),
                ])),
            }],
            ..Structure::default()
        };
        let too_big = capture_to_upload(
            instance.path(),
            "Alex",
            "build",
            Some(3),
            Access::for_player(None),
            |access| {
                assert!(access == Access::Ordinary);
                Some(fixture)
            },
        );
        assert!(matches!(too_big, Err(UploadError::TooLarge(n)) if n > UPLOAD_BYTES));
        assert!(dir.occupied(oldest).unwrap());
        assert!(!dir.occupied(file_leaf("build_1.nbt").unwrap()).unwrap());
        assert_eq!(
            dir.regular_files(sch::valid_file).unwrap().len(),
            sch::MAX_SCHEMATICS
        );

        let receipt = capture_to_upload(
            instance.path(),
            "Alex",
            "build",
            Some(3),
            Access::for_player(Some(true)),
            |access| {
                assert!(access == Access::GameMaster);
                Some(Structure {
                    size: [2, 1, 1],
                    ..Structure::default()
                })
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            (receipt.slot, receipt.file.as_str(), receipt.size),
            (3, "build_1.nbt", [2, 1, 1])
        );
        assert!(!dir.occupied(oldest).unwrap());
        assert_eq!(
            dir.regular_files(sch::valid_file).unwrap().len(),
            sch::MAX_SCHEMATICS
        );
        let saved = read_bounded(&dir, file_leaf(&receipt.file).unwrap()).unwrap();
        assert_eq!(
            decode_structure(&receipt.file, &saved).unwrap().size,
            receipt.size
        );
        assert_eq!(
            read_bounded(&root, file_leaf("build.nbt").unwrap()).unwrap(),
            b"independent"
        );
    }

    #[test]
    fn table_listing_keeps_root_and_own_saved_files_separate() {
        let instance = TempDir::new("create-table-listing");
        let root = folder(instance.path(), true).unwrap();
        let alex = owner_folder(&root, owner_leaf("Alex").unwrap(), true).unwrap();
        let steve = owner_folder(&root, owner_leaf("Steve").unwrap(), true).unwrap();
        let name = file_leaf("same.nbt").unwrap();
        root.write_new(name, b"root").unwrap();
        alex.write_new(name, b"alex").unwrap();
        steve
            .write_new(file_leaf("other.nbt").unwrap(), b"steve")
            .unwrap();
        assert_eq!(
            list_files(instance.path(), Some("Alex")),
            [
                TableFile::Local("same.nbt".into()),
                TableFile::Saved("same.nbt".into()),
            ]
        );
        assert_eq!(
            list_files(instance.path(), None),
            [TableFile::Local("same.nbt".into())]
        );
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
