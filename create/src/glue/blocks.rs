//! Create's blocks that a vanilla template mostly runs: valve handles (a crank turned by an
//! angle, recoloured by dye), sliding doors (the iron door's template with Create's double doors,
//! `visible` and the sliding panel), toolboxes (recoloured by dye with their items kept, their
//! menu's screen laid out as compartments), postboxes (`open` while their menu is) and the train
//! and framed glass trapdoors. Rules live in `create_engine`.
//!
//! The toolboxes' block entity type, `create:toolbox`, is processing's, with the slots;
//! their menu's screen and its recolouring are here. A postbox's packages are the packages
//! port's (`port_stacks`).

use super::server::{ServerLevel, UPDATE_CLIENTS, update_all};
use super::states;
use super::subsystem::Subsystem;
use super::world::with_world;
use crate::glue::names::Named as _;
use create_engine::Block;
use create_engine::door::{Door, Slide};
use create_engine::dye::{self, DyeColor};
use mod_api::{
    BlockPart, BlockState, Compound, Hand, Host, HostSide, HostTransaction, InteractionResult,
    MenuPart, PlayerContainerCloseEvent, PlayerRef, Registrar, RegistrationContext, ServerHost,
    SoundSource, UseItemOnEvent, UseWithoutItemEvent,
};

use super::processing::TOOLBOX_TYPE;

pub struct Blocks;

impl Subsystem for Blocks {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        for &block in Block::ALL.iter() {
            if dye::is_valve_handle(block)
                || dye::is_sliding_door(block)
                || dye::is_toolbox(block)
                || dye::is_postbox(block)
                || matches!(block, Block::TrainTrapdoor | Block::FramedGlassTrapdoor)
            {
                r.block_part(states::key(block), TemplateBlock);
            }
        }
        let menu = create_engine::toolbox::MENU;
        if let Err(e) = r.register_menu_screen(menu, std::sync::Arc::new(toolbox_screen)) {
            crate::create_log!("[create] the toolbox screen was refused: {e}");
        }
        r.menu_part(menu, ToolboxButtons);
    }

    fn level_tick(&self, lv: &mut ServerLevel<'_>) {
        tick_doors(lv);
    }

    fn screen_render_post(
        &self,
        host: &dyn mod_api::ClientHost,
        event: &mut mod_api::ScreenRenderPostEvent<'_>,
    ) {
        toolbox_totals(host, event);
    }

    /// `PostboxBlockEntity.stopOpen`: the last menu closed on a postbox shuts it.
    fn player_container_close(
        &self,
        host: &mut dyn ServerHost,
        event: &mut PlayerContainerCloseEvent<'_>,
    ) {
        let Some(window) = event.menu.and_then(|menu| menu.container_id(&*host)) else {
            return;
        };
        let Some(Some((dim, at))) = with_world(|w| w.postboxes.closed(event.player.uuid(), window))
        else {
            return;
        };
        let Some(dimension) = super::world::level_id(dim) else {
            return;
        };
        if let Some(mut lv) = ServerLevel::new(host, &dimension) {
            set_postbox_open(&mut lv, at, false);
        }
    }
}

/// The Create block at `pos`, its state facts and its state.
pub(crate) fn block_at(
    lv: &ServerLevel<'_>,
    pos: [i32; 3],
) -> Option<(Block, create_engine::parts::StateInfo, BlockState)> {
    let state = lv.block_state(pos)?;
    let id = states::id(state)?;
    let (b, info) = with_world(|w| w.table.get(id).map(|(b, i)| (b, *i))).flatten()?;
    Some((b, info, state))
}

/// `state` with `props` set.
fn with_props(host: &dyn Host, state: BlockState, props: &[(&str, &str)]) -> Option<BlockState> {
    let facts = mod_api::HostBlockStates::block_states(host);
    props
        .iter()
        .try_fold(state, |s, (k, v)| s.with(&facts, k, v).ok())
}

/// `BlockHelper.copyProperties(state, to.getDefaultState())`: block `to` with the state's
/// properties.
pub(crate) fn recoloured_state(
    host: &dyn Host,
    state: BlockState,
    to: Block,
) -> Option<BlockState> {
    copied_state(host, state, states::key(to))
}

/// `BlockHelper.copyProperties(state, to.defaultBlockState())` for any block `to`: the
/// properties `to` has take the state's values, the rest keep their defaults.
pub(crate) fn copied_state(
    host: &dyn Host,
    state: BlockState,
    to: mod_api::BlockKey,
) -> Option<BlockState> {
    let text = host.block_state_string(state)?;
    let (_, props) = states::split(&text);
    let base = host.block_state_default(to)?;
    let facts = mod_api::HostBlockStates::block_states(host);
    Some(
        props
            .iter()
            .fold(base, |s, (k, v)| s.with(&facts, k, v).unwrap_or(s)),
    )
}

fn sneaking(host: &dyn Host, player: PlayerRef) -> bool {
    player.entity().is_shift_key_down(host) == Some(true)
}

fn play(lv: &mut ServerLevel<'_>, sound: &str, pos: [i32; 3], volume: f32, pitch: f32) {
    let at = pos.map(|v| f64::from(v) + 0.5);
    lv.level.play_sound(
        &mut *lv.host,
        sound,
        at,
        SoundSource::Blocks,
        volume,
        pitch,
        None,
    );
}

/// A pitch of `random.nextFloat() * 0.1 + 0.9`, from the level's random.
fn door_pitch(lv: &mut ServerLevel<'_>) -> f32 {
    lv.level
        .with_random(&mut *lv.host, |r| r.next_float())
        .map_or(0.9, |f| f * 0.1 + 0.9)
}

/// The valve handles, sliding doors, toolboxes, postboxes and Create's trapdoors.
struct TemplateBlock;

#[mod_api::sunrise_part]
impl BlockPart for TemplateBlock {
    fn use_item_on(
        &self,
        host: HostSide<'_>,
        event: &mut UseItemOnEvent<'_>,
    ) -> Option<InteractionResult> {
        let HostSide::Server(host) = host else {
            return None;
        };
        let click = event.click;
        let player = host.player(click.player)?;
        let dimension = click.dimension.key().id().to_owned();
        let mut lv = ServerLevel::new(host, &dimension)?;
        let (block, _, state) = block_at(&lv, click.pos)?;
        let back = sneaking(&*lv.host, player);
        let item = click.stack.map(|s| s.item().name(&*lv.host));
        if dye::is_valve_handle(block) {
            use_valve_handle(
                &mut lv,
                player,
                click.pos,
                block,
                state,
                item.as_deref(),
                back,
            )
        } else if dye::is_toolbox(block) {
            use_toolbox(&mut lv, click.pos, block, state, item.as_deref(), back)
        } else {
            None
        }
    }

    fn use_without_item(
        &self,
        host: HostSide<'_>,
        event: &mut UseWithoutItemEvent<'_>,
    ) -> Option<InteractionResult> {
        let HostSide::Server(host) = host else {
            return None;
        };
        let click = event.click;
        let player = host.player(click.player)?;
        let dimension = click.dimension.key().id().to_owned();
        let mut lv = ServerLevel::new(host, &dimension)?;
        let (block, info, state) = block_at(&lv, click.pos)?;
        let back = sneaking(&*lv.host, player);
        if dye::is_sliding_door(block) {
            use_door(&mut lv, click.pos, block, &info)
        } else if dye::is_postbox(block) {
            use_postbox(&mut lv, player, click.pos, block, click.hand, back)
        } else if matches!(block, Block::TrainTrapdoor | Block::FramedGlassTrapdoor) {
            use_trapdoor(&mut lv, click.pos, state, info.open)
        } else {
            None
        }
    }
}

/// `ValveHandleBlock.onBlockActivated` → `clicked`: not for a player who may not build, nor a
/// sneaking wrench; a dye of another colour recolours it (`DYED_VALVE_HANDLES`, its properties
/// kept), else it turns (`activate`). The click is taken either way.
pub(crate) fn use_valve_handle(
    lv: &mut ServerLevel<'_>,
    player: PlayerRef,
    pos: [i32; 3],
    block: Block,
    state: BlockState,
    item: Option<&str>,
    back: bool,
) -> Option<InteractionResult> {
    if player.may_build(&*lv.host) != Some(true) {
        return None;
    }
    if back && item == Some("create:wrench") {
        return None;
    }
    if let Some(to) = item
        .and_then(DyeColor::of_item)
        .and_then(|c| dye::recoloured(block, c))
    {
        if let Some(next) = recoloured_state(&*lv.host, state, to) {
            // `ValveHandleBlock.onRemove`: another valve handle keeps the block entity, and so
            // its angle.
            let kept = lv
                .host
                .block_entity(lv.level, pos)
                .and_then(|e| e.data(&*lv.host))
                .and_then(|d| {
                    let key = super::sources::handles::SCROLL_VALUE;
                    Some(Compound::from([(key.into(), d.get(&key.into())?.clone())]))
                });
            if !lv.set_state(pos, next, update_all()) {
                return None;
            }
            if let (Some(kept), Some(entity)) = (kept, lv.host.block_entity(lv.level, pos)) {
                let _ = entity.merge_data(&mut *lv.host, kept);
            }
        }
        // ValveHandleBlock.clicked keeps the entity; publish its cleared kinetic
        // fields and the shafts' live state before a same-tick save.
        super::persist::publish(lv);
        return Some(InteractionResult::Success);
    }
    let dim = lv.dim;
    let entity = lv.host.block_entity(lv.level, pos);
    let value = entity
        .and_then(|e| e.data(&*lv.host))
        .map_or(create_engine::valve::DEFAULT_ANGLE, |d| {
            super::sources::handles::valve_angle(&d)
        });
    if with_world(|w| w.use_valve(dim, pos, back, value)) == Some(true)
        && let Some(entity) = entity
    {
        let used = super::sources::handles::valve_use(lv.game_time(), value, back);
        let _ = entity.merge_data(&mut *lv.host, used);
    }
    super::persist::publish(lv);
    Some(InteractionResult::Success)
}

/// `ToolboxBlock.useItemOn`: sneaking passes (empty-handed, nothing opens); a dye of another
/// colour recolours it, its items kept (`onRemove` keeps the block entity for another
/// toolbox); else its menu opens (the template's use).
fn use_toolbox(
    lv: &mut ServerLevel<'_>,
    pos: [i32; 3],
    block: Block,
    state: BlockState,
    item: Option<&str>,
    back: bool,
) -> Option<InteractionResult> {
    if back {
        return item.is_none().then_some(InteractionResult::Success);
    }
    let to = item
        .and_then(DyeColor::of_item)
        .and_then(|c| dye::recoloured(block, c))?;
    let Some(next) = recoloured_state(&*lv.host, state, to) else {
        return Some(InteractionResult::Success);
    };
    let slots = create_engine::toolbox::STORED as u32;
    let items = snapshot_toolbox(lv.host.block_entity(lv.level, pos), slots, |be, i| {
        be.item_slot(&*lv.host, i)
    })?;
    recolour_toolbox(
        lv,
        items,
        |lv| lv.set_state(pos, next, update_all()),
        |lv| {
            lv.set_state(pos, state, update_all());
        },
        |lv, items| {
            let be = lv.host.block_entity(lv.level, pos).ok_or(())?;
            lv.host
                .transaction(|host, tx| {
                    for (i, stack) in items {
                        let existing = be.item_slot(&*host, *i).map_err(|_| ())?;
                        restore_slot(existing, stack, |put| {
                            be.set_item_slot(host, tx, *i, Some(put))
                        })?;
                    }
                    Ok::<(), ()>(())
                })
                .map_err(|_| ())
        },
        |lv, (_, stack)| lv.drop_stack(pos.map(|v| f64::from(v) + 0.5), [0.0; 3], stack, 10),
    )
    .then_some(InteractionResult::Success)
}

/// The occupied slots of the source toolbox, in index order. A missing block entity or a slot
/// that cannot be read is `None`: the toolbox is not known to be intact or empty, so nothing
/// may be recoloured.
fn snapshot_toolbox<B, T, E>(
    entity: Option<B>,
    slots: u32,
    mut read: impl FnMut(&B, u32) -> Result<Option<T>, E>,
) -> Option<Vec<(u32, T)>> {
    let be = entity?;
    let mut items = Vec::new();
    for i in 0..slots {
        if let Some(stack) = read(&be, i).ok()? {
            items.push((i, stack));
        }
    }
    Some(items)
}

/// Put a captured `stack` back into a slot that held `existing` now. An empty slot is written
/// (`write` reporting a changed or unchanged slot alike is fine; an `Err` aborts). The identical
/// stack already there was preserved in place and needs no write. A different occupant is not
/// the captured stack: it is never overwritten and the restore fails, so rollback and salvage
/// run and drop whatever was not recovered.
fn restore_slot<T: PartialEq + Clone, E>(
    existing: Option<T>,
    stack: &T,
    write: impl FnOnce(T) -> Result<bool, E>,
) -> Result<(), ()> {
    match existing {
        None => write(stack.clone()).map(|_| ()).map_err(|_| ()),
        Some(there) if there == *stack => Ok(()),
        Some(_) => Err(()),
    }
}

/// Try the full snapshot atomically. After a rollback attempt, salvage each stack independently
/// so one refused slot does not send the rest to the ground.
fn recolour_toolbox<W, T>(
    world: &mut W,
    items: Vec<T>,
    recolour: impl FnOnce(&mut W) -> bool,
    rollback: impl FnOnce(&mut W),
    mut restore: impl FnMut(&mut W, &[T]) -> Result<(), ()>,
    mut drop_stack: impl FnMut(&mut W, T),
) -> bool {
    if !recolour(world) {
        return false;
    }
    if items.is_empty() || restore(world, &items).is_ok() {
        return true;
    }
    rollback(world);
    for stack in items {
        if restore(world, std::slice::from_ref(&stack)).is_err() {
            drop_stack(world, stack);
        }
    }
    false
}

/// `PostboxBlock.useWithoutItem` → `PackagePortBlockEntity.use`: the main hand's click, not
/// sneaking, opens its screen (packages' `port_screen`); it shows open until the screen closes
/// (`startOpen`, `stopOpen`).
fn use_postbox(
    lv: &mut ServerLevel<'_>,
    player: PlayerRef,
    pos: [i32; 3],
    block: Block,
    hand: Hand,
    back: bool,
) -> Option<InteractionResult> {
    if back || hand != Hand::Main {
        return None;
    }
    let _ = block;
    if super::packages::port_clipboard(lv, player, pos) {
        return Some(InteractionResult::Success);
    }
    super::packages::port_screen::open(lv, player, pos, false);
    set_postbox_open(lv, pos, true);
    Some(InteractionResult::Success)
}

/// The port screen of the postbox at `pos` closed: it shuts.
pub(crate) fn postbox_closed(lv: &mut ServerLevel<'_>, pos: [i32; 3]) {
    set_postbox_open(lv, pos, false);
}

/// `PostboxBlockEntity.onOpenChange`: `open` set (`setBlockAndUpdate`) with a barrel's sound, if
/// the block is still a postbox.
fn set_postbox_open(lv: &mut ServerLevel<'_>, pos: [i32; 3], open: bool) {
    if !block_at(lv, pos).is_some_and(|(b, _, _)| dye::is_postbox(b)) {
        return;
    }
    let value = if open { "true" } else { "false" };
    lv.set_property(pos, "open", value, update_all());
    let sound = if open {
        "minecraft:block.barrel.open"
    } else {
        "minecraft:block.barrel.close"
    };
    play(lv, sound, pos, 1.0, 1.0);
}

/// `TrainTrapdoorBlock.useWithoutItem`: a click opens or closes it whatever its power
/// (`state.cycle(OPEN)`, `UPDATE_CLIENTS`), with its `BlockSetType`'s iron trapdoor sounds.
fn use_trapdoor(
    lv: &mut ServerLevel<'_>,
    pos: [i32; 3],
    state: BlockState,
    open: bool,
) -> Option<InteractionResult> {
    let open = !open;
    let next = with_props(
        &*lv.host,
        state,
        &[("open", if open { "true" } else { "false" })],
    )?;
    lv.set_state(pos, next, UPDATE_CLIENTS);
    let sound = if open {
        "minecraft:block.iron_trapdoor.open"
    } else {
        "minecraft:block.iron_trapdoor.close"
    };
    let pitch = door_pitch(lv);
    play(lv, sound, pos, 1.0, pitch);
    Some(InteractionResult::Success)
}

/// The sliding door at `pos`: its block, door and state.
fn door_at(lv: &ServerLevel<'_>, pos: [i32; 3]) -> Option<(Block, Door, BlockState)> {
    let (b, info, state) = block_at(lv, pos)?;
    dye::is_sliding_door(b)
        .then(|| Door::of(&info))
        .flatten()
        .map(|d| (b, d, state))
}

/// Both halves of the door at `pos` set `open` (and hidden while open), as `setBlock` of one
/// half and the other's `updateShape` (which copies it, `visible` too) leave them.
fn set_door_open(lv: &mut ServerLevel<'_>, pos: [i32; 3], door: &Door, open: bool) {
    for at in [pos, door.other_half(pos)] {
        let Some((_, _, state)) = door_at(lv, at) else {
            continue;
        };
        let props: &[(&str, &str)] = if open {
            &[("open", "true"), ("visible", "false")]
        } else {
            &[("open", "false")]
        };
        if let Some(next) = with_props(&*lv.host, state, props).filter(|s| *s != state) {
            // UPDATE_CLIENTS | UPDATE_IMMEDIATE
            lv.set_state(at, next, UPDATE_CLIENTS);
        }
    }
}

/// `SlidingDoorBlock.useWithoutItem`: the click opens or closes the door; a double door's other
/// door follows (`tick_doors`), and an iron door's opening sound plays at the last door it
/// reached.
fn use_door(
    lv: &mut ServerLevel<'_>,
    pos: [i32; 3],
    block: Block,
    info: &create_engine::parts::StateInfo,
) -> Option<InteractionResult> {
    let door = Door::of(info)?;
    let open = !door.open;
    set_door_open(lv, pos, &door, open);
    if !open {
        return Some(InteractionResult::Success);
    }
    let partner = door.partner(pos);
    // `isDoubleDoor(state.cycle(OPEN), ...)`: the other door not yet as open as this one now.
    let double = door_at(lv, partner)
        .is_some_and(|(b, other, _)| b == block && Door { open, ..door }.is_double(&other));
    let at = if double { partner } else { pos };
    let pitch = door_pitch(lv);
    play(lv, "minecraft:block.iron_door.open", at, 1.0, pitch);
    Some(InteractionResult::Success)
}

/// Sliding doors after the level's tick: a door that opened is hidden (`visible` false, as
/// Create sets it with `open`) and takes the other door of a double door with it, and its panel
/// slides; once a closed door's panel has slid shut it shows again, with an iron door's
/// closing sound. Only doors that changed or still slide cost anything.
fn tick_doors(lv: &mut ServerLevel<'_>) {
    let dim = lv.dim;
    let idle = |w: &mut create_engine::world::CreateWorld| {
        w.dims[dim].door_changes.is_empty() && w.dims[dim].doors.is_empty()
    };
    if with_world(idle) != Some(false) {
        return;
    }
    // A double door's other door changes in this pass too.
    for _ in 0..4 {
        let changes =
            with_world(|w| std::mem::take(&mut w.dims[dim].door_changes)).unwrap_or_default();
        if changes.is_empty() {
            break;
        }
        for (pos, was_open) in changes {
            let Some((block, door, state)) = door_at(lv, pos) else {
                continue;
            };
            if door.open
                && door.visible
                && let Some(next) = with_props(&*lv.host, state, &[("visible", "false")])
            {
                lv.set_state(pos, next, UPDATE_CLIENTS);
            }
            let partner = door.partner(pos);
            if let Some((b, other, _)) = door_at(lv, partner)
                && b == block
                && door.is_double(&other)
            {
                set_door_open(lv, partner, &other, door.open);
            }
            if !door.upper {
                with_world(|w| {
                    w.dims[dim]
                        .doors
                        .entry(pos)
                        .or_insert_with(|| Slide::at_rest(was_open));
                });
            }
        }
    }
    let moving: Vec<[i32; 3]> =
        with_world(|w| w.dims[dim].doors.keys().copied().collect()).unwrap_or_default();
    for pos in moving {
        let Some((_, door, state)) = door_at(lv, pos).filter(|(_, d, _)| !d.upper) else {
            with_world(|w| w.dims[dim].doors.remove(&pos));
            continue;
        };
        let Some((show, done)) = with_world(|w| {
            let s = w.dims[dim].doors.get_mut(&pos)?;
            let show = s.tick(door.open, door.visible);
            Some((show, s.at_target()))
        })
        .flatten() else {
            continue;
        };
        if show {
            // `showBlockModel`: UPDATE_ALL, the upper half following through `updateShape`.
            for (at, s) in [(pos, Some(state)), (door.other_half(pos), None)] {
                let s = s.or_else(|| door_at(lv, at).map(|(_, _, s)| s));
                if let Some(next) = s.and_then(|s| with_props(&*lv.host, s, &[("visible", "true")]))
                {
                    lv.set_state(at, next, update_all());
                }
            }
            play(lv, "minecraft:block.iron_door.close", pos, 0.5, 1.0);
        }
        if done {
            with_world(|w| w.dims[dim].doors.remove(&pos));
        }
    }
}

/// A toolbox's screen (`ToolboxScreen`) shows each compartment by its first stack round the
/// toolbox's picture; the other stacks and the container's four slots past Create's 32 are
/// hidden (the server's menu leaves them out too).
fn toolbox_screen(
    _host: &dyn mod_api::ClientHost,
    open: mod_api::MenuScreenOpen<'_>,
) -> Option<mod_api::MenuScreen> {
    use create_engine::toolbox;
    let name = open.block?.to_owned();
    let block = toolbox_block(&name)?;
    let layout = mod_api::MenuScreenLayout {
        width: toolbox::SCREEN_WIDTH,
        height: toolbox::SCREEN_HEIGHT,
        slots: (0..toolbox::SLOTS)
            .map(|i| {
                toolbox::slot_position(i).map(|position| mod_api::ScreenSlot {
                    position,
                    no_item_icon: None,
                })
            })
            .collect(),
        inventory_y: toolbox::INVENTORY_Y,
        inventory_x: toolbox::INVENTORY_X,
        title_label: None,
        inventory_label: None,
        own_labels: true,
        own_background: true,
        recipe_book: None,
        recipe_book_toggle: None,
        recipe_book_big_result: false,
    };
    Some(
        mod_api::MenuScreen::new(layout)
            .with_draw(move |host, frame, draw| toolbox_draw(host, &name, block, frame, draw))
            // The deposit button (`ToolboxDisposeAllPacket`) and the confirm button (closes).
            .with_click(|_host, frame, right| {
                if right {
                    None
                } else if on_button(TOOLBOX_DEPOSIT, frame.at) {
                    Some(TOOLBOX_DEPOSIT_BUTTON)
                } else if on_button(toolbox_confirm(), frame.at) {
                    Some(TOOLBOX_CONFIRM_BUTTON)
                } else {
                    None
                }
            })
            // The deposit button's `create.toolbox.depositBox`.
            .with_tooltip(|_host, frame, _slot, lines| {
                if on_button(TOOLBOX_DEPOSIT, frame.at) {
                    lines.push((
                        mod_api::Text::translate("create.toolbox.depositBox", vec![]),
                        0xFFFF_FFFF,
                    ));
                }
            }),
    )
}

/// `ToolboxScreen.renderBg`: the toolbox's background with its name, the player's inventory
/// under it, the deposit button and the confirm button.
fn toolbox_draw(
    host: &dyn mod_api::ClientHost,
    name: &str,
    block: Block,
    frame: mod_api::MenuScreenFrame<'_>,
    draw: &mut dyn mod_api::DrawContext,
) {
    use create_engine::toolbox::{BACKGROUND, SCREEN_HEIGHT};
    let [ox, oy] = frame.origin;
    let bx = ox + 30.0;
    draw.sprite(
        "create:gui/toolbox",
        [0.0, 0.0, BACKGROUND[0], BACKGROUND[1]],
        [bx, oy, BACKGROUND[0], BACKGROUND[1]],
        None,
    );
    let title = host
        .translate(&format!("block.create.{}", block.name()))
        .unwrap_or_default();
    draw.text(&title, [bx + 15.0, oy + 4.0], 0xFF59_2424, false);
    let iy = oy + SCREEN_HEIGHT - 108.0;
    draw.sprite(
        "create:gui/player_inventory",
        [0.0, 0.0, 176.0, 108.0],
        [ox, iy, 176.0, 108.0],
        None,
    );
    draw.text(
        &host.translate("container.inventory").unwrap_or_default(),
        [ox + 8.0, iy + 6.0],
        0xFF40_4040,
        false,
    );
    // `renderToolbox`: the toolbox, 50 px a block, right of the background's foot.
    if let Some(state) = host.block_state_id(name) {
        let _ = draw.draw_scene(
            [
                bx + BACKGROUND[0],
                oy + 116.0,
                bx + BACKGROUND[0] + 72.0,
                oy + 184.0,
            ],
            &mod_api::Scene::new(vec![mod_api::SceneBlock::new([0, 0, 0], state, None)]),
            mod_api::SceneView::new([0.5, 0.5, 0.5], -202.0, 22.0, 50.0),
        );
    }
    for (at, icon) in [(TOOLBOX_DEPOSIT, I_TOOLBOX), (toolbox_confirm(), I_CONFIRM)] {
        let (x, y) = (ox + at[0], oy + at[1]);
        draw.sprite(
            "create:gui/widgets",
            [0.0, 0.0, 18.0, 18.0],
            [x, y, 18.0, 18.0],
            None,
        );
        draw.sprite(
            "create:gui/icons",
            [icon[0], icon[1], 16.0, 16.0],
            [x + 1.0, y + 1.0, 16.0, 16.0],
            None,
        );
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ToolboxAction {
    DepositAll,
    Confirm,
}

impl ToolboxAction {
    fn decode(menu: &str, block: Option<&str>, id: i32) -> Option<Self> {
        if menu != create_engine::toolbox::MENU || block.and_then(toolbox_block).is_none() {
            return None;
        }
        match id {
            TOOLBOX_DEPOSIT_BUTTON => Some(Self::DepositAll),
            TOOLBOX_CONFIRM_BUTTON => Some(Self::Confirm),
            _ => None,
        }
    }
}

/// Server-side buttons for a toolbox's menu.
struct ToolboxButtons;

#[mod_api::sunrise_part]
impl MenuPart for ToolboxButtons {
    fn menu_button_valid(
        &self,
        _host: &dyn Host,
        event: &mut mod_api::MenuButtonValidEvent<'_>,
    ) -> mod_api::Flow {
        if ToolboxAction::decode(event.menu, event.block, event.id).is_some() {
            mod_api::Flow::Cancel
        } else {
            mod_api::Flow::Continue
        }
    }

    fn menu_button(&self, event: &mut mod_api::MenuButtonEvent<'_>) {
        let Some(action) = ToolboxAction::decode(event.menu, event.block, event.context.id) else {
            return;
        };
        if event.subject.with_view(|v| v.id() == TOOLBOX_TYPE) != Some(true) {
            return;
        }
        match action {
            ToolboxAction::Confirm => {
                crate::glue::close_menu(event.subject.host(), event.context.player)
            }
            ToolboxAction::DepositAll => deposit_all(event),
        }
    }
}

/// `ToolboxScreen.renderBg`: each compartment shows its first stack with the compartment's total
/// (`totalCountInCompartment`) for its count. The screen's slots draw their own stack's count, so
/// over a compartment holding more than its first stack the slot is drawn again: the
/// background under it, the stack with the total, and the hover highlight. Not while a stack is
/// carried over it.
fn toolbox_totals(host: &dyn mod_api::ClientHost, event: &mut mod_api::ScreenRenderPostEvent<'_>) {
    use create_engine::toolbox::{STACKS_PER_COMPARTMENT, STORED};
    let Some(pos) = host.screen_pos() else {
        return;
    };
    let toolbox = host
        .level(host.dimension())
        .and_then(|l| l.block_state(host, pos))
        .map(states::id_or_none)
        .and_then(|id| super::client_table(host, |t| t.get(id).map(|(b, _)| b)).flatten())
        .is_some_and(dye::is_toolbox);
    if !toolbox {
        return;
    }
    let Some(menu) = host.local_player().and_then(|p| p.container_menu(host)) else {
        return;
    };
    let Some(boxes) = menu.slot_boxes(host) else {
        return;
    };
    let carried = menu.carried(host).flatten().is_some();
    let mouse = event.screen.mouse;
    let [ox, oy] = [event.screen.rect[0], event.screen.rect[1]];
    for first in (0..STORED).step_by(STACKS_PER_COMPARTMENT) {
        let Some(Some([x, y, w, h])) = boxes.get(first).copied() else {
            continue;
        };
        let Some(stack) = menu.slot(host, first).flatten() else {
            continue;
        };
        let total: u32 = (first..first + STACKS_PER_COMPARTMENT)
            .filter_map(|i| menu.slot(host, i).flatten())
            .map(|s| s.count())
            .sum();
        if total == stack.count() {
            continue;
        }
        let hovered = (x..x + w).contains(&mouse[0]) && (y..y + h).contains(&mouse[1]);
        if hovered && carried {
            continue;
        }
        let draw = &mut *event.draw;
        // The background under the slot (`AllGuiTextures.TOOLBOX`, 30 px in).
        draw.sprite(
            "create:gui/toolbox",
            [x - ox - 30.0, y - oy, 16.0, 16.0],
            [x, y, 16.0, 16.0],
            None,
        );
        draw.gui_draw_item_with(&stack, [x, y], Some(&total.to_string()));
        if hovered {
            draw.fill([x, y, 16.0, 16.0], 0x80FF_FFFF);
        }
    }
}

/// `AllIcons.I_TOOLBOX` and `I_CONFIRM` in `icons.png`.
const I_TOOLBOX: [f32; 2] = [176.0, 48.0];
const I_CONFIRM: [f32; 2] = [0.0, 16.0];
/// The deposit button, `(30 + 81, 69)` in the window.
const TOOLBOX_DEPOSIT: [f32; 2] = [111.0, 69.0];
const TOOLBOX_DEPOSIT_BUTTON: i32 = 0;
const TOOLBOX_CONFIRM_BUTTON: i32 = 1;

/// The toolbox named by a screen's `block`, if it is one.
fn toolbox_block(block: &str) -> Option<Block> {
    block
        .strip_prefix("create:")
        .and_then(Block::from_name)
        .filter(|b| dye::is_toolbox(*b))
}

/// Whether `at` is on the 18 px button at `button`.
fn on_button(button: [f32; 2], at: [f32; 2]) -> bool {
    (button[0]..button[0] + 18.0).contains(&at[0]) && (button[1]..button[1] + 18.0).contains(&at[1])
}

/// The confirm button: `30 + BG.width - 33`, `BG.height - 24`.
fn toolbox_confirm() -> [f32; 2] {
    let bg = create_engine::toolbox::BACKGROUND;
    [30.0 + bg[0] - 33.0, bg[1] - 24.0]
}

/// `ToolboxDisposeAllPacket`: each of the player's 36 stacks goes into the compartment that
/// already holds its kind (`inLimitedMode`), onto its stacks and then its empty stacks
/// (`insertItemStacked`).
fn deposit_all(event: &mut mod_api::MenuButtonEvent<'_>) {
    use create_engine::toolbox::{STACKS_PER_COMPARTMENT, STORED};
    let who = event.context.player;
    // Every write on both sides is accepted or none stays: a refusal aborts the transaction, which
    // restores the player's slots and the toolbox's.
    let moved = event.subject.transaction(|scope, _| {
        let mut items = scope
            .with_view(|v| {
                (0..STORED as u32)
                    .map(|i| v.item_slot(i).and_then(|s| s.to_owned()))
                    .collect::<Vec<_>>()
            })
            .ok_or(())?;
        let host = scope.host();
        let player = host.player(who).ok_or(())?;
        let slots = player
            .with_inventory(&*host, |view| {
                view.iter()
                    .take_while(|(slot, _)| *slot < 36)
                    .filter_map(|(slot, stack)| Some((slot, stack.project()?)))
                    .collect::<Vec<_>>()
            })
            .ok_or(())?;
        let mut changed = vec![false; items.len()];
        for (slot, stack) in slots {
            let mut left = stack.count();
            let max = host.max_stack_size(stack.item()).unwrap_or(64);
            for c in 0..STORED / STACKS_PER_COMPARTMENT {
                let range = c * STACKS_PER_COMPARTMENT..(c + 1) * STACKS_PER_COMPARTMENT;
                // The compartment's kind: its first stack that holds anything.
                let kind = range.clone().find_map(|i| items.get(i).cloned().flatten());
                if !kind.is_some_and(|k| k.item() == stack.item() && k.same_components(&stack)) {
                    continue;
                }
                for i in range.clone() {
                    if let Some(Some(held)) = items.get_mut(i) {
                        let room = max.saturating_sub(held.count()).min(left);
                        if room > 0 {
                            held.set_count(held.count() + room).map_err(|_| ())?;
                            left -= room;
                            changed[i] = true;
                        }
                    }
                }
                for i in range {
                    if left == 0 {
                        break;
                    }
                    if let Some(empty @ None) = items.get_mut(i) {
                        let n = left.min(max);
                        let mut put = stack.clone();
                        put.set_count(n).map_err(|_| ())?;
                        left -= n;
                        *empty = Some(put);
                        changed[i] = true;
                    }
                }
                break;
            }
            if left != stack.count() {
                let rest = remainder(stack, left, |s, n| s.set_count(n))?;
                player
                    .set_inventory_slot(host, slot, rest)
                    .map_err(|_| ())?;
            }
        }
        scope
            .with_view(|mut v| {
                for (i, stack) in items.into_iter().enumerate() {
                    if changed[i] {
                        v.set_item_slot(i as u32, stack).map_err(|_| ())?;
                    }
                }
                Ok::<_, ()>(v.handle())
            })
            .ok_or(())?
    });
    // `setChanged` is not journaled, so only a committed transfer issues it.
    if let Ok(entity) = moved {
        let _ = entity.set_changed(event.subject.host());
    }
}

/// What stays in the player's slot after a deposit: nothing when `left` is 0, else `rest` with
/// that count. A refused count is an `Err`, which aborts the transaction, never an emptied slot.
fn remainder<T, E>(
    mut rest: T,
    left: u32,
    set_count: impl FnOnce(&mut T, u32) -> Result<(), E>,
) -> Result<Option<T>, ()> {
    if left == 0 {
        return Ok(None);
    }
    set_count(&mut rest, left).map_err(|_| ())?;
    Ok(Some(rest))
}

#[cfg(test)]
mod toolbox_button_tests {
    use super::*;

    #[test]
    fn the_remainder_is_nothing_the_stack_or_a_failed_transaction() {
        let set = |s: &mut (&str, u32), n| {
            s.1 = n;
            Ok::<(), ()>(())
        };
        assert_eq!(remainder(("iron", 9), 0, set), Ok(None));
        assert_eq!(remainder(("iron", 9), 3, set), Ok(Some(("iron", 3))));
        let refused = |_: &mut (&str, u32), _| Err::<(), _>("refused");
        assert_eq!(remainder(("iron", 9), 3, refused), Err(()));
        // Nothing is left over, so the count is never set.
        assert_eq!(remainder(("iron", 9), 0, refused), Ok(None));
    }

    #[test]
    fn restore_slot_writes_only_an_empty_slot_and_accepts_only_the_same_stack() {
        assert_eq!(restore_slot(None, &"iron", |_| Ok::<_, ()>(true)), Ok(()));
        assert_eq!(restore_slot(None, &"iron", |_| Ok::<_, ()>(false)), Ok(()));
        assert_eq!(
            restore_slot(None, &"iron", |_| Err::<bool, _>("refused")),
            Err(())
        );
        let never = |_| -> Result<bool, ()> { panic!("an occupied slot is never written") };
        assert_eq!(restore_slot(Some("iron"), &"iron", never), Ok(()));
        assert_eq!(restore_slot(Some("stone"), &"iron", never), Err(()));
    }

    #[test]
    fn an_unrelated_occupant_fails_the_restore_and_the_captured_stack_is_dropped() {
        // (slots, rolled back, dropped): slot 1 was refilled by something else.
        let mut world = (vec![None, Some("stone")], false, Vec::new());
        let items = vec![(0, "iron"), (1, "copper")];
        assert!(!recolour_toolbox(
            &mut world,
            items,
            |_| true,
            |(_, rolled_back, _)| *rolled_back = true,
            |(slots, _, _), items| {
                // Atomic like the transaction: nothing stays when any write fails.
                let mut trial = slots.clone();
                for (i, stack) in items {
                    let existing = trial[*i];
                    restore_slot(existing, stack, |put| {
                        trial[*i] = Some(put);
                        Ok::<_, ()>(true)
                    })?;
                }
                *slots = trial;
                Ok(())
            },
            |(_, rolled_back, dropped), (_, stack)| {
                assert!(*rolled_back);
                dropped.push(stack);
            },
        ));
        let (slots, rolled_back, dropped) = world;
        assert!(rolled_back);
        // The iron is salvaged into its empty slot, the copper is dropped, never skipped, and
        // the unrelated occupant is untouched.
        assert_eq!(slots, [Some("iron"), Some("stone")]);
        assert_eq!(dropped, ["copper"]);
    }

    #[test]
    fn refused_recolour_leaves_the_toolbox_alone() {
        let mut stored = ["iron", "copper"];
        let snapshot = stored.to_vec();
        assert!(!recolour_toolbox(
            &mut stored,
            snapshot,
            |_| false,
            |_| panic!("nothing changed to roll back"),
            |_, _| panic!("the original contents are still there"),
            |_, _| panic!("nothing needs dropping"),
        ));
        assert_eq!(stored, ["iron", "copper"]);
    }

    #[test]
    fn successful_recolour_restores_the_full_snapshot_without_drops() {
        let mut stored = vec!["iron", "copper"];
        let snapshot = stored.clone();
        assert!(recolour_toolbox(
            &mut stored,
            snapshot.clone(),
            |stored| {
                stored.clear();
                true
            },
            |_| panic!("the restore succeeded"),
            |stored, items| {
                assert!(stored.is_empty());
                assert_eq!(items, snapshot.as_slice());
                stored.extend_from_slice(items);
                Ok(())
            },
            |_, _| panic!("everything was restored"),
        ));
        assert_eq!(stored, snapshot);
    }

    #[test]
    fn failed_restore_rolls_back_and_drops_only_unrecovered_stacks() {
        for can_salvage_iron in [true, false] {
            let mut world = (false, Vec::new(), Vec::new());
            assert!(!recolour_toolbox(
                &mut world,
                vec!["iron", "copper"],
                |_| true,
                |(rolled_back, _, _)| *rolled_back = true,
                |(rolled_back, stored, _), items| {
                    if !*rolled_back {
                        assert_eq!(items, ["iron", "copper"]);
                        return Err(());
                    }
                    if can_salvage_iron && items == ["iron"] {
                        stored.extend_from_slice(items);
                        Ok(())
                    } else {
                        Err(())
                    }
                },
                |(rolled_back, _, dropped), stack| {
                    assert!(*rolled_back);
                    dropped.push(stack);
                },
            ));
            let (rolled_back, stored, dropped) = world;
            assert!(rolled_back);
            if can_salvage_iron {
                assert_eq!(stored, ["iron"]);
                assert_eq!(dropped, ["copper"]);
            } else {
                assert!(stored.is_empty());
                assert_eq!(dropped, ["iron", "copper"]);
            }
        }
    }

    #[test]
    fn an_unreadable_source_toolbox_aborts_before_any_recolour() {
        let recolour = |items: Vec<(u32, &'static str)>| {
            recolour_toolbox(
                &mut (),
                items,
                |_| panic!("an unreadable toolbox is never recoloured"),
                |_| panic!("nothing changed to roll back"),
                |_, _| panic!("nothing to restore"),
                |_, _| panic!("nothing to drop"),
            )
        };
        // No block entity: not known to be empty, `read` is never called.
        let missing = snapshot_toolbox(None::<()>, 4, |_, _| -> Result<Option<&str>, ()> {
            panic!("no entity to read")
        });
        assert_eq!(missing, None);
        assert_eq!(missing.map(recolour), None);
        // A slot that cannot be read, even after readable ones.
        let failing = snapshot_toolbox(Some(()), 4, |_, i| match i {
            0 => Ok(Some("iron")),
            2 => Err("unreadable"),
            _ => Ok(None),
        });
        assert_eq!(failing, None);
        assert_eq!(failing.map(recolour), None);
        // A clean read keeps the occupied slots in index order.
        let stacks = [Some("iron"), None, Some("copper"), None];
        let items = snapshot_toolbox(Some(stacks), 4, |be, i| Ok::<_, ()>(be[i as usize]));
        assert_eq!(items, Some(vec![(0, "iron"), (2, "copper")]));
    }

    #[test]
    fn the_shared_toolbox_menu_only_decodes_deposit_and_confirm() {
        let menu = create_engine::toolbox::MENU;
        for block in Block::ALL
            .iter()
            .copied()
            .filter(|block| dye::is_toolbox(*block))
        {
            let name = format!("create:{}", block.name());
            let decode = |id| ToolboxAction::decode(menu, Some(&name), id);
            assert_eq!(
                decode(TOOLBOX_DEPOSIT_BUTTON),
                Some(ToolboxAction::DepositAll)
            );
            assert_eq!(decode(TOOLBOX_CONFIRM_BUTTON), Some(ToolboxAction::Confirm));
            for id in [i32::MIN, 2, 100, i32::MAX] {
                assert_eq!(decode(id), None);
            }
        }
        for (menu, block) in [
            (menu, None),
            (menu, Some("create:schematicannon")),
            ("create:schematicannon", Some("create:red_toolbox")),
        ] {
            assert_eq!(
                ToolboxAction::decode(menu, block, TOOLBOX_DEPOSIT_BUTTON),
                None
            );
        }
    }
}
