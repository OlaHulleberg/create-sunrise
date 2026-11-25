//! What Create keeps for each column of a level that belongs to no block entity (trains,
//! superglue...): one chunk attachment holding a compound, with a key per subsystem. Block entity
//! fields are the entities' own (`entity_data`).
//!
//! The game saves a chunk's attachments with it, and saves the chunk before it unloads. There
//! is no moment just before a chunk is saved to write an attachment in, so a column whose state
//! changed is written into its attachment, and its block entities' fields published, at the end
//! of the level tick that changed it and when an interaction is accepted. A column that did not
//! change is not written. A block entity's own save (`block_entity_save_additional`) refreshes
//! the fields that are numbers from the engine as it is then (`Subsystem::live_fields`); fields
//! that need the item registry stay as last published.

use super::server::ServerLevel;
use super::subsystem::Subsystem;
use mod_api::{AttachmentCodec, AttachmentDescriptor, AttachmentKey, Compound, Holder, Registrar};
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::sync::RwLock;

/// The column's data. Saved under `create:column`; the key is issued when the mod registers.
static COLUMN: RwLock<Option<AttachmentKey<Compound>>> = RwLock::new(None);

/// Declares the column attachment (`Hooks::register`) and keeps its key.
pub fn register(r: &mut dyn Registrar) {
    let descriptor =
        AttachmentDescriptor::new("create:column", Compound::new).serialize(AttachmentCodec {
            encode: |data| Ok(data.clone()),
            decode: |data| Ok(data.clone()),
        });
    match r.register_attachment_type(descriptor) {
        Ok(key) => {
            if let Ok(mut slot) = COLUMN.write() {
                *slot = Some(key);
            }
        }
        Err(e) => crate::create_log!("[create] the column attachment was refused: {e:?}"),
    }
}

/// The issued column key, once the mod has registered.
fn column_key() -> Option<AttachmentKey<Compound>> {
    COLUMN.read().ok().and_then(|slot| slot.clone())
}

thread_local! {
    /// Columns whose data changed since they were last written, by level index.
    static UNSAVED: RefCell<BTreeSet<(usize, [i32; 2])>> = const { RefCell::new(BTreeSet::new()) };
}

/// Marks column `chunk` of level `dim` to be written at the end of this tick.
pub fn mark_unsaved(dim: usize, chunk: [i32; 2]) {
    UNSAVED.with_borrow_mut(|u| {
        u.insert((dim, chunk));
    });
}

/// Marks the column holding `pos`.
pub fn mark_pos_unsaved(dim: usize, pos: [i32; 3]) {
    mark_unsaved(dim, [pos[0] >> 4, pos[2] >> 4]);
}

/// Forgets every mark (the world closed).
pub fn clear() {
    UNSAVED.with_borrow_mut(BTreeSet::clear);
}

/// The saved data of column `chunk` of `lv`, read when it loads. Empty for a column Create
/// never wrote.
pub fn read(lv: &mut ServerLevel<'_>, chunk: [i32; 2]) -> Compound {
    let Some(key) = column_key() else {
        return Compound::new();
    };
    match key.get_existing(&mut *lv.host, Holder::Chunk(chunk), |data| data.cloned()) {
        Ok(Some(data)) => data,
        Ok(None) => Compound::new(),
        Err(e) => {
            crate::create_log!("[create] column {chunk:?} of {}: {e:?}", lv.dimension);
            Compound::new()
        }
    }
}

/// Column keys of data the block entities now keep: read once from an older save, which is then
/// written again without them.
pub const LEGACY_KEYS: &[&str] = &[
    "motors",
    "create_equipment",
    "create_owners",
    "links",
    "diodes",
    "create_redstone",
    "arms",
    "ejectors",
    "vaults",
    "chute_filters",
    "tunnel_filters",
    "filter_amounts",
    "belts",
    "brass",
    "chutes",
    "funnels",
    "tank_entities",
    "tank_multis",
    "tanks",
    "boilers",
    "create_fluids",
    "create_machines",
    "Connections",
    "open_ends",
    "deployers",
    "crushers",
    "burners",
    "basins",
    "crafter_inputs",
    "crafter_covers",
    "create_controller",
    "create_display",
    "create_decor",
    "create_copycat",
    "track_curves",
    "create_packages",
    "create_stock",
    "create_table_cloths",
    "create_schematics",
    "windmills",
    "pistons",
    "gantries",
    "modes",
    "chassis",
    "create_lifts",
    "create_contacts",
];

/// Whether column data `data` holds what an older version kept there for block entities.
pub fn holds_legacy(data: &Compound) -> bool {
    LEGACY_KEYS.iter().any(|k| data.contains_key(&(*k).into()))
}

/// Writes every marked column of `lv` that is still loaded, asking each subsystem for its part.
pub fn flush(lv: &mut ServerLevel<'_>, systems: &[&dyn Subsystem]) {
    let dim = lv.dim;
    let columns: Vec<[i32; 2]> = UNSAVED.with_borrow_mut(|u| {
        let mine: Vec<_> = u.iter().filter(|(d, _)| *d == dim).copied().collect();
        for key in &mine {
            u.remove(key);
        }
        mine.into_iter().map(|(_, c)| c).collect()
    });
    let Some(key) = column_key() else {
        return;
    };
    for chunk in columns {
        let loaded = lv
            .level
            .chunk_ref(&*lv.host, chunk)
            .is_some_and(|c| c.loaded(&*lv.host));
        if !loaded {
            continue;
        }
        super::entity_data::save(lv, chunk);
        let mut data = Compound::new();
        for s in systems {
            s.chunk_data_save(lv, chunk, &mut data);
        }
        let result = if data.is_empty() {
            key.remove(&mut *lv.host, Holder::Chunk(chunk)).map(|_| ())
        } else {
            key.set(&mut *lv.host, Holder::Chunk(chunk), data)
        };
        if let Err(e) = result {
            crate::create_log!(
                "[create] writing column {chunk:?} of {}: {e:?}",
                lv.dimension
            );
        }
    }
}

/// Complete Create's synchronous destroyBlock effects, then publish live records
/// and flush columns. Shared by accepted interactions and completed level ticks;
/// no entity tick or kinetic solve runs here.
pub(crate) fn publish(lv: &mut ServerLevel<'_>) {
    super::kinetics::drain_effects(lv);
    super::entity_data::note_clock(lv);
    for (cx, cz) in super::world::with_world(|w| w.take_unsaved(lv.dim)).unwrap_or_default() {
        mark_unsaved(lv.dim, [cx, cz]);
    }
    for s in super::SUBSYSTEMS {
        s.write_entities(lv);
    }
    flush(lv, super::SUBSYSTEMS);
}
