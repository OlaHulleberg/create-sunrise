//! What schematics read and write of the game beyond the hooks: the `schematics/` files (gzip
//! NBT, as `NbtIo` reads and writes them, through the engine's `gzip` and the API's NBT
//! encoding), a schematic's data on its stack (`minecraft:custom_data`) and each block's
//! item (`Block.asItem`).
//!
//! The files live in the mod's game data folder (`sunrise-mod-data/create/schematics/`): the
//! API reaches a mod's own folders only, not the game folder's `schematics/` where Create keeps
//! them.

use crate::glue::names::Named as _;
use crate::glue::packages::body::{CustomData, set_custom};
use create_engine::gzip;
use create_engine::schematic::{Nbt, SchematicData, SchematicUpload, UploadRetention};
use mod_api::{Host, ItemStack, Tag};
use std::collections::HashMap;
use std::io;
use std::path::{Component, Path, PathBuf};

/// `SchematicItem.loadSchematic`'s `NbtAccounter.create(0x20000000L)`.
const MAX_BYTES: usize = 0x2000_0000;

/// The folder under the mod's data root.
const ROOT: &str = "schematics";

/// `rel` under the mod's bulk data folder (`<instance>/sunrise-mod-data/create/schematics`).
/// Names come from clients, so only plain components are taken: no `..`, root or prefix.
fn path(host: &dyn Host, rel: &str) -> Option<PathBuf> {
    let rel = Path::new(rel);
    let plain =
        !rel.as_os_str().is_empty() && rel.components().all(|c| matches!(c, Component::Normal(_)));
    plain.then(|| folder(host).join(rel))
}

fn folder(host: &dyn Host) -> PathBuf {
    host.instance_dir()
        .join("sunrise-mod-data")
        .join("create")
        .join(ROOT)
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

/// A structure file under `schematics/` (`NbtIo.read` of the gzip stream), once per load.
pub fn read_structure(host: &dyn Host, rel: &str) -> Option<Nbt> {
    let bytes = std::fs::read(path(host, rel)?).ok()?;
    let raw = match gzip::decode(&bytes, MAX_BYTES) {
        Ok(raw) => raw,
        Err(e) => {
            crate::create_log!("[create] Failed to read schematic {rel}: {e:?}");
            return None;
        }
    };
    match read_named(&raw) {
        Some(t) => Some(to_engine(&t)),
        None => {
            crate::create_log!("[create] Failed to read schematic {rel}: not NBT");
            None
        }
    }
}

/// `NbtIo.writeCompressed` to a file under `schematics/` (its folders made, an old file
/// replaced); its size.
pub fn write_structure(host: &dyn Host, rel: &str, tag: &Nbt) -> Result<u64, String> {
    let file = path(host, rel).ok_or("not a file name")?;
    let raw = write_named(&to_game(tag)).ok_or("not writable as NBT")?;
    let gz = gzip::encode(&raw);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&file, &gz).map_err(|e| e.to_string())?;
    Ok(gz.len() as u64)
}

/// The schematics saved in `schematics/` (`ClientSchematicLoader.refresh`: its `.nbt` files, by
/// name).
pub fn local_files(host: &dyn Host) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(folder(host)) else {
        return Vec::new();
    };
    let mut out: Vec<String> = entries
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.ends_with(".nbt"))
        .collect();
    out.sort();
    out
}

pub fn exists(host: &dyn Host, rel: &str) -> bool {
    path(host, rel).is_some_and(|p| p.exists())
}

pub fn size(host: &dyn Host, rel: &str) -> Option<u64> {
    let meta = std::fs::metadata(path(host, rel)?).ok()?;
    meta.is_file().then(|| meta.len())
}

/// Store a local schematic as its owner's upload, evicting the oldest at the cap.
/// Table uploads exclude the destination name before counting; instant uploads include it.
pub(super) fn upload(
    host: &dyn Host,
    from: &str,
    to: &str,
    upload: SchematicUpload,
) -> io::Result<()> {
    let invalid = || io::Error::new(io::ErrorKind::InvalidInput, "not a file name");
    let from = path(host, from).ok_or_else(invalid)?;
    let to = path(host, to).ok_or_else(invalid)?;
    let upload_folder = to.parent().ok_or_else(invalid)?;
    let name = to
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(invalid)?;
    let bytes = std::fs::read(&from)?;

    let mut retention = UploadRetention::default();
    match std::fs::read_dir(upload_folder) {
        Ok(entries) => {
            for entry in entries.flatten() {
                let Ok(meta) = entry.metadata() else {
                    continue;
                };
                let Ok(entry_name) = entry.file_name().into_string() else {
                    continue;
                };
                if meta.is_file() {
                    let modified = meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map_or(0, |d| d.as_millis() as u64);
                    retention.record(upload, name, entry_name, modified);
                }
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    if let Some(oldest) = retention.evict() {
        std::fs::remove_file(upload_folder.join(oldest))?;
    }
    std::fs::create_dir_all(upload_folder)?;
    std::fs::write(&to, &bytes)
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
    use super::{read_named, to_engine, to_game, write_named};
    use create_engine::gzip;
    use create_engine::schematic::Nbt;
    use mod_api::Tag;

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
