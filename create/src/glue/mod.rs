//! The game glue: Create's engine driven through the mod API. Each subsystem is one module;
//! `lib.rs` hands every hook to all of them through `SUBSYSTEMS`.

pub(crate) mod advancements;
pub mod blocks;
pub mod contraptions;
pub mod copycat;
pub mod cracks;
mod data_dir;
pub mod decor;
pub mod display;
pub(crate) mod drawn;
pub mod entity_data;
pub mod equipment;
pub mod fluids;
pub mod gauges;
pub mod interaction;
pub mod items;
pub mod keys;
pub mod kinetics;
#[macro_use]
pub mod log;
pub mod logistics;
pub mod models;
pub mod names;
pub mod net;
pub mod owned;
pub mod packages;
pub mod persist;
pub mod pipelines;
pub mod processing;
pub mod redstone;
pub mod schematics;
pub mod sequencer;
pub mod server;
pub mod sources;
pub mod states;
pub mod stock;
pub mod subsystem;
pub mod trains;
pub mod transmission;
pub mod world;

#[cfg(test)]
#[path = "../test_support/temp_dir.rs"]
mod temp_dir;

use create_engine::world::StateTable;
use mod_api::text::TextContent;
use mod_api::{ClientHost, Host, ItemStack, ServerHost, Text};
use server::ServerLevel;
use std::cell::RefCell;
use std::collections::HashSet;
use subsystem::Subsystem;

/// Resolve a stack's hover name in the caller's language for plain labels and search.
pub(crate) fn item_name(host: &dyn Host, stack: &ItemStack) -> String {
    host.hover_name(stack)
        .map(|name| host.text_string(&name))
        .unwrap_or_default()
}

/// Keep the name as a translation argument: its style and any literal percent signs are data.
pub(crate) fn item_name_message(
    key: &str,
    fallback: Option<&str>,
    mut name: Text,
    count: Option<u32>,
) -> Text {
    if let Some(count) = count {
        name = name.append(Text::literal(format!(" x{count}")));
    }
    Text::new(TextContent::Translatable {
        key: key.to_owned(),
        fallback: fallback.map(str::to_owned),
        arguments: vec![name],
    })
}

/// Every subsystem, in the order their level ticks run.
///
/// Before the kinetic solve, so that what they change turns in this tick's solve:
/// - `Equipment`: backtanks fill (first, ahead of Create's own tick).
/// - `Kinetics`: cranks and valves turn down, water wheels, clutches and gearshifts follow
///   redstone, chain drives, sequenced gearshifts. It declares the `create:kinetic` block entity
///   type in `register`, so whatever attaches to `kinetics::kind()` there comes after it.
/// - `Blocks`: sliding doors (postboxes open and shut at their menu, not here).
/// - `Redstone`: latches, controller rails, diodes, lamps, cuckoo clocks, links, then the
///   contents touched since the last tick.
/// - `Controller` (linked controllers), `Display` (display links), `Contraptions` (bearings,
///   their actors, gantries, windmills, stickers, pistons, mounted interfaces, lifts) and its
///   `Drawing`.
/// - `Processing`: what comes before the machines' turns (deployers' redstone, nozzles, crushing
///   wheel controllers, blaze burners).
/// - `Fluids`: pipes and pumps, boilers, valves and hoses, tank states, spouts, drains.
/// - `Carts`, then `Trains` (the controls, the trains, the HUD, curves) and `TrainControls`.
/// - `Logistics`: vaults, chutes, then the level's block-entity walk: every block entity in
///   `Level.blockEntityTickers` order, its `KineticBlockEntity.tick` first, then its own turn
///   (pipes, tanks, spouts, drains, funnels, tunnels, depots, belts whose machines answer
///   `belt_process`, presses, mixers, basins, saws, drills, deployers, millstones, fans,
///   crushers, crafters, sequenced gearshifts and cranks); and `Entities`, its part of the
///   kinetic type.
/// - `processing::Tools`: what the machines' turns leave for clients, and the entities inside
///   turning saws and drills.
/// - `Arms`: mechanical arms, weighted ejectors.
/// - `redstone::Detectors`: smart observers and threshold switches, reading this tick's tank
///   changes before `fluids::Tanks` hands them on (and drains them).
/// - `kinetics::Solve`: the base ticks of block entities placed since the walk, the network
///   is solved, speeds go to clients.
///
/// After the solve:
/// - `Gauges`: dials the solve moved.
/// - `Packages`, `Stock`, `Schematics`, `Decor`, `Seats`, `Copycats`, `Items`: the
///   subsystems that tick after Create's own.
/// - `Comparators`, last: comparators beside every block whose output changed read it again.
///
/// The same order is the order the shared moments ask them in: the wheel goes to the train
/// controls before the value boxes; a left click to decor before the items; a placement to the
/// equipment before the items; the kinetic world hears of a block change before the others.
pub static SUBSYSTEMS: &[&dyn Subsystem] = &[
    &equipment::Equipment,
    &items::potato::PotatoCannon,
    &kinetics::Kinetics,
    &blocks::Blocks,
    &redstone::Redstone,
    &contraptions::controller::Controller,
    &display::Display,
    &contraptions::Contraptions,
    &contraptions::draw::Drawing,
    &processing::Processing,
    &fluids::Fluids,
    &trains::Carts,
    &trains::Trains,
    &trains::TrainControls,
    &contraptions::colliders::Colliders,
    &interaction::Interaction,
    &logistics::Logistics,
    &logistics::Entities,
    &processing::Tools,
    &logistics::Arms,
    &redstone::Detectors,
    &fluids::Tanks,
    &kinetics::Solve,
    &gauges::Gauges,
    &sources::Sources,
    &packages::Packages,
    &stock::Stock,
    &schematics::Schematics,
    &decor::Decor,
    &contraptions::seats::Seats,
    &copycat::Copycats,
    &items::Items,
    &redstone::Comparators,
    &advancements::Owners,
];

thread_local! {
    /// The columns of each level whose saved data the subsystems have been given.
    static COLUMNS: RefCell<HashSet<(usize, [i32; 2])>> = RefCell::new(HashSet::new());
    /// The client's copy of the engine's state table, for drawing (a client of a remote server
    /// has no server thread of its own to build it).
    static CLIENT_TABLE: RefCell<Option<StateTable>> = const { RefCell::new(None) };
}

/// A block entity being drawn (`block_entity_submit`, or one the glue draws itself riding a
/// structure).
pub struct Submit<'a> {
    pub pos: [i32; 3],
    pub block: &'a str,
    pub state: mod_api::BlockState,
    pub entity: EntityData<'a>,
    pub draw: &'a mut dyn mod_api::BlockEntityDrawContext,
}

/// The saved data of the block entity a draw reads: the event's borrowed view, or a structure's
/// carried copy, which the API cannot lend as a view.
#[derive(Clone, Copy)]
pub enum EntityData<'a> {
    /// The data the game lends a callback.
    View(mod_api::BlockEntityDataView<'a>),
    /// A structure's carried copy.
    Carried(&'a mod_api::Compound),
}

impl<'a> EntityData<'a> {
    pub fn new(nbt: &'a mod_api::Compound) -> Self {
        Self::Carried(nbt)
    }

    pub fn nbt(&self) -> &mod_api::Compound {
        match self {
            Self::View(view) => view.nbt(),
            Self::Carried(nbt) => nbt,
        }
    }
}

impl<'a> From<mod_api::BlockEntityDataView<'a>> for EntityData<'a> {
    fn from(view: mod_api::BlockEntityDataView<'a>) -> Self {
        Self::View(view)
    }
}

impl<'a> Submit<'a> {
    pub fn of(event: &'a mut mod_api::BlockEntitySubmitEvent<'_>) -> Submit<'a> {
        Submit {
            pos: event.pos,
            block: event.block,
            state: event.state,
            entity: event.entity.into(),
            draw: &mut *event.draw,
        }
    }
}

/// `ConnectivityHandler.isConnected` for connected textures: the block at `pos` joins the blocks
/// of the multiblock whose controller is `controller` (`BlockPos.asLong`), none for None.
pub fn connection_group(
    lv: &mut server::ServerLevel<'_>,
    pos: [i32; 3],
    controller: Option<[i32; 3]>,
) {
    let group = controller.map(|[x, y, z]| {
        ((i64::from(x) & 0x3FF_FFFF) << 38
            | (i64::from(z) & 0x3FF_FFFF) << 12
            | (i64::from(y) & 0xFFF)) as u64
    });
    let level = lv.level;
    let _ = level.set_connection_group(&mut *lv.host, pos, group);
}

/// Registers one of Create's particle types (`ICustomParticleData.createType`, which does not
/// override the limiter). Each particle keeps its own state, so the options carry nothing.
pub fn particle_type(r: &mut dyn mod_api::Registrar, id: &str) -> mod_api::ParticleType<()> {
    r.register_particle_type(
        id,
        false,
        mod_api::ParticleMapCodec::unit(),
        mod_api::ParticleStreamCodec::wire(),
    )
}

/// The server's block entity of a `block_entity_on_load`, and the host to reach it with.
pub fn loaded_entity<'a>(
    subject: &'a mut mod_api::BlockEntitySideScope<'_>,
) -> Option<(mod_api::BlockEntityRef, &'a mut dyn mod_api::ServerHost)> {
    let mod_api::BlockEntitySideScope::Server(scope) = subject else {
        return None;
    };
    let entity = scope.with_view(|v| v.handle())?;
    Some((entity, scope.host()))
}

/// Gives the subsystems column `chunk`'s saved data, once while it is loaded: when the column
/// loads, or earlier when one of its block entities loads first.
pub fn ensure_column(host: &mut dyn ServerHost, dimension: &str, chunk: [i32; 2]) {
    let Some(mut lv) = ServerLevel::new(host, dimension) else {
        return;
    };
    if !COLUMNS.with_borrow_mut(|c| c.insert((lv.dim, chunk))) {
        return;
    }
    load_column(&mut lv, chunk);
}

fn load_column(lv: &mut ServerLevel<'_>, chunk: [i32; 2]) {
    let data = persist::read(lv, chunk);
    for s in SUBSYSTEMS {
        s.chunk_load(lv, chunk, &data);
    }
    let legacy = persist::holds_legacy(&data);
    if legacy {
        persist::mark_unsaved(lv.dim, chunk);
    }
    entity_data::load(lv, chunk, legacy);
}

/// Has the subsystems read the data block entities of `lv` loaded since they last asked: a
/// column's when it comes in, then each one `/data` or a placement gives a block entity.
pub fn load_entities(lv: &mut ServerLevel<'_>) {
    for chunk in entity_data::pending_columns(lv) {
        if COLUMNS.with_borrow_mut(|c| c.insert((lv.dim, chunk))) {
            load_column(lv, chunk);
        } else {
            entity_data::load(lv, chunk, false);
        }
    }
}

/// Column `chunk` of level `dim` left: the next load reads its data again.
pub fn forget_column(dim: usize, chunk: [i32; 2]) {
    COLUMNS.with_borrow_mut(|c| c.remove(&(dim, chunk)));
}

/// Forgets every column (the world closed).
pub fn forget_columns() {
    COLUMNS.with_borrow_mut(HashSet::clear);
}

/// Runs `f` on the client's state table, building it on first use. `None` when Create's blocks
/// are not in the game.
pub fn client_table<R>(host: &dyn Host, f: impl FnOnce(&StateTable) -> R) -> Option<R> {
    CLIENT_TABLE.with_borrow_mut(|t| {
        if t.is_none() {
            *t = states::open(host);
        }
        t.as_ref().map(f)
    })
}

/// The client left its world.
pub fn client_reset(_host: &mut dyn ClientHost) {
    CLIENT_TABLE.with_borrow_mut(|t| *t = None);
    models::forget();
    cracks::forget();
    for s in SUBSYSTEMS {
        s.client_reset();
    }
}

/// Whether `block` is in the block tag named `tag` (`minecraft:rails`): the tag's key is looked
/// up in `tags` by its name, and `None` when the name or the tag does not exist.
pub fn block_in_tag(tags: &dyn mod_api::Tags, block: mod_api::BlockKey, tag: &str) -> Option<bool> {
    tags.block_in_tag(
        block,
        tags.key(&mod_api::Identifier::parse(tag).ok()?)?.typed(),
    )
}

/// The block-local boxes of `state`'s `kind` shape at `pos` (`BlockState.getShape`,
/// `getCollisionShape`), none when the level or state is unavailable.
pub fn block_shape(
    level: mod_api::LevelRef,
    host: &dyn mod_api::Host,
    state: mod_api::BlockState,
    pos: [i32; 3],
    kind: mod_api::BlockShapeKind,
) -> Option<Vec<[f64; 6]>> {
    let mut boxes = Vec::new();
    level
        .visit_block_shape(host, state, pos, kind, &mut |b| {
            boxes.push(b);
            std::ops::ControlFlow::Continue(())
        })
        .map(|_| boxes)
}

/// The world-space boxes of the block collision shapes meeting `bounds`
/// (`CollisionGetter.getBlockCollisions`), none when the level is unavailable.
pub fn collision_boxes(
    level: mod_api::LevelRef,
    host: &dyn mod_api::Host,
    bounds: [f64; 6],
) -> Option<Vec<[f64; 6]>> {
    let mut boxes = Vec::new();
    level
        .visit_block_collisions(host, bounds, &mut |shape| {
            boxes.extend(shape.boxes());
            std::ops::ControlFlow::Continue(())
        })
        .map(|_| boxes)
}

/// The entities riding `entity`, none when it cannot be read.
pub fn passengers(
    entity: mod_api::EntityRef,
    host: &dyn mod_api::Host,
) -> Option<Vec<mod_api::EntityRef>> {
    let mut riders = Vec::new();
    entity
        .visit_passengers(host, &mut |rider| {
            riders.push(rider);
            std::ops::ControlFlow::Continue(())
        })
        .ok()
        .map(|_| riders)
}

/// `living`'s modifiers on `attribute`, none when it cannot be read; one whose id does not parse
/// is left out.
pub fn attribute_modifiers(
    living: mod_api::LivingRef,
    host: &dyn mod_api::Host,
    attribute: &str,
) -> Option<Vec<mod_api::AttributeModifier>> {
    let mut modifiers = Vec::new();
    living
        .visit_attribute_modifiers(host, attribute, &mut |m| {
            if let Ok(id) = mod_api::Identifier::parse(m.id) {
                modifiers.push(mod_api::AttributeModifier::new(id, m.amount, m.operation));
            }
            std::ops::ControlFlow::Continue(())
        })
        .ok()
        .map(|_| modifiers)
}

/// The menu `uuid`'s player has open, when its window is `window`.
pub fn open_menu(host: &dyn mod_api::Host, uuid: u128, window: i32) -> Option<mod_api::MenuRef> {
    let menu = host.player(uuid)?.container_menu(host)?;
    (menu.container_id(host) == Some(window)).then_some(menu)
}

/// Closes the menu `uuid`'s player has open (`ServerPlayer.closeContainer`).
pub fn close_menu(host: &mut dyn mod_api::ServerHost, uuid: u128) {
    if let Some(menu) = host
        .player(uuid)
        .and_then(|player| player.container_menu(&*host))
    {
        menu.close_container(host);
    }
}

#[cfg(test)]
mod item_name_tests {
    use super::*;
    use mod_api::text::TextStyle;

    #[test]
    fn item_name_message_keeps_the_style_and_count() {
        let mut style = TextStyle::default();
        style.color = Some("#9E57CE".into());
        style.italic = Some(true);
        let name = Text::literal("Cogwheel").with_style(style);
        let message = item_name_message("create.send", Some("Send %1$s"), name.clone(), Some(4));
        assert_eq!(
            message.content,
            TextContent::Translatable {
                key: "create.send".into(),
                fallback: Some("Send %1$s".into()),
                arguments: vec![name.append(Text::literal(" x4"))],
            }
        );
    }

    #[test]
    fn item_name_message_never_treats_an_items_percent_signs_as_placeholders() {
        let name = Text::literal("Cog %s %1$s");
        let message = item_name_message("create.send", Some("Send %s"), name.clone(), None);
        assert_eq!(
            message.content,
            TextContent::Translatable {
                key: "create.send".into(),
                fallback: Some("Send %s".into()),
                arguments: vec![name],
            }
        );
    }
}
