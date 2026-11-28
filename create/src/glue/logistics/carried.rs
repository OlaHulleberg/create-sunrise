//! What logistics keeps for a block that rides a moving structure (`Subsystem::carry_out`,
//! Create's block entity NBT): the filter slot (`FilteringBehaviour`: `Filter`, `FilterAmount`,
//! `UpTo`) of a smart chute, a brass funnel, a basin, a saw or a deployer; a brass tunnel's side
//! filters (`SidedFilteringBehaviour`, by side: no `transform` turns them in Create either),
//! its selection mode and the stack it distributes; a belt's items (`BeltBlockEntity.write`'s
//! `Inventory`: the whole belt moves with any of its segments, `Contraption.moveBelt`), kept by
//! the first of its segments taken, with where its controller was from there.

use super::{brass_from_nbt, brass_to_nbt, tunnel_filters_from_nbt, tunnel_filters_to_nbt};
use crate::glue::items::filters;
use crate::glue::server::ServerLevel;
use crate::glue::world::with_world;
use create_engine::Block;
use mod_api::{Compound, Tag};

pub(crate) const FILTER: &str = "Filter";
const AMOUNT: &str = "FilterAmount";
const SIDES: &str = "SideFilters";
const BRASS: &str = "BrassTunnel";
/// The angle a depot's held stack lies at (`HeldItem`'s `Angle`), for drawing it moving.
pub(crate) const DEPOT_ANGLE: &str = "DepotAngle";
/// A belt segment riding (so its renderer draws it: `BeltRenderer` in `renderBlockEntities`).
pub(super) const BELT_SEGMENT: &str = "BeltSegment";
/// The belt's items (`belts_to_nbt`), its controller at `pos` from the segment.
const BELT_ITEMS: &str = "BeltItems";
/// The belt as its clients draw it (`belt::encode`), its controller as an offset from the
/// segment, for drawing its items moving.
pub(super) const BELT_SHOWN: &str = "BeltShown";

pub(super) fn carry_out(lv: &ServerLevel<'_>, pos: [i32; 3], block: Block, out: &mut Compound) {
    let dim = lv.dim;
    let host = &*lv.host;
    if block == Block::Belt {
        carry_belt(dim, pos, out);
    }
    let Some((filter, amount, sides, brass)) = with_world(|w| {
        let d = &mut w.dims[dim];
        if let Some(Some(h)) = d.depots.get(&pos) {
            out.insert(DEPOT_ANGLE.into(), Tag::Int(h.angle));
        }
        let filter = d.chute_filters.remove(&pos);
        let amount = d.filter_amounts.remove(&pos);
        let sides: Vec<_> = create_engine::kinetics::Dir::ALL
            .into_iter()
            .filter_map(|s| Some(((pos, s), d.tunnel_filters.remove(&(pos, s))?)))
            .collect();
        let brass = (block == Block::BrassTunnel)
            .then(|| d.brass.remove(&pos))
            .flatten();
        (filter, amount, sides, brass)
    }) else {
        return;
    };
    if let Some(t) = filter
        .as_ref()
        .and_then(|f| filters::filter_to_nbt(host, f))
    {
        out.insert(FILTER.into(), t);
    }
    if let Some(a) = amount {
        out.insert(
            AMOUNT.into(),
            Tag::IntArray(vec![i32::from(a.count), i32::from(a.up_to)]),
        );
    }
    if !sides.is_empty() {
        let mut c = Compound::new();
        tunnel_filters_to_nbt(host, &mut c, &sides);
        out.insert(SIDES.into(), Tag::Compound(c));
    }
    if let Some(b) = brass {
        out.insert(BRASS.into(), brass_to_nbt(&[([0; 3], b.to_saved())]));
    }
}

pub(super) fn carry_in(lv: &ServerLevel<'_>, pos: [i32; 3], block: Block, data: &Compound) {
    let dim = lv.dim;
    let host = &*lv.host;
    let filter = data
        .get(&FILTER.into())
        .and_then(|t| filters::filter_from_nbt(host, t));
    let amount = match data.get(&AMOUNT.into()) {
        Some(Tag::IntArray(v)) if v.len() == 2 => Some(create_engine::board::FilterAmount::new(
            v[0].clamp(0, 255) as u8,
            v[1] != 0,
        )),
        _ => None,
    };
    let sides = match data.get(&SIDES.into()) {
        Some(Tag::Compound(c)) => tunnel_filters_from_nbt(host, c),
        _ => Vec::new(),
    };
    let brass = match data.get(&BRASS.into()) {
        Some(t) if block == Block::BrassTunnel => {
            brass_from_nbt(&Compound::from([("brass".into(), t.clone())]))
                .into_iter()
                .next()
                .map(|(_, b)| b)
        }
        _ => None,
    };
    with_world(|w| {
        let d = &mut w.dims[dim];
        if let Some(f) = filter {
            d.chute_filters.insert(pos, f);
        }
        if let Some(a) = amount {
            d.filter_amounts.insert(pos, a);
        }
        for ((_, side), f) in sides {
            d.tunnel_filters.insert((pos, side), f);
        }
        if let Some(b) = brass {
            w.read_brass(dim, pos, b);
        }
    });
}

/// The carried record may ride on any segment, but Java loads Inventory only on the controller.
/// A controller this landing wrote that heads a whole belt chain takes its items; for any other
/// cell (refused controller, non-controller segment, broken or partial belt chain, or overflowing
/// offset) they come back as drops, each with where it falls.
///
/// `placed` gates only the controller, while `whole_chain` reads the live world (`level_state`)
/// for the rest of the chain. That is deliberate: `initialize_belt` forms a belt whenever a newly
/// written `Part::Start` controller heads a complete live chain, including when an end or middle
/// segment already stood in the world and its own write was refused, and it consumes
/// `belt_saved` at the controller. A written controller whose chain is incomplete, or whose
/// block is not a start, cannot form a belt, so its items drop; a controller this landing did
/// not write never consumes them either.
pub(crate) fn restore_belt_items(
    world: &mut create_engine::world::CreateWorld,
    dim: usize,
    pos: [i32; 3],
    data: &Compound,
    turn: create_engine::contraption::Transform,
    placed: impl Fn([i32; 3]) -> bool,
    level_state: impl Fn([i32; 3]) -> u32,
) -> Vec<([i32; 3], create_engine::belt::Item)> {
    let Some(Tag::List(list)) = data.get(&BELT_ITEMS.into()) else {
        return Vec::new();
    };
    let saved = Compound::from([("belts".into(), Tag::List(list.clone()))]);
    let mut drops = Vec::new();
    for (offset, items) in super::belts_from_nbt(&saved) {
        let controller = turn.checked_vec(offset).and_then(|o| checked_add(pos, o));
        if let Some(controller) = controller
            && placed(controller)
            && create_engine::belt::whole_chain(controller, |p| {
                world.table.belt_segment(level_state(p))
            })
            .is_some()
        {
            world.read_belt_items(dim, controller, items);
        } else {
            // A malformed offset that overflows has no controller: it never writes the world,
            // and its items fall at the anchor.
            let drop_pos = controller.unwrap_or(pos);
            drops.extend(items.items.into_iter().map(|item| (drop_pos, item)));
        }
    }
    drops
}

fn checked_add(a: [i32; 3], b: [i32; 3]) -> Option<[i32; 3]> {
    Some([
        a[0].checked_add(b[0])?,
        a[1].checked_add(b[1])?,
        a[2].checked_add(b[2])?,
    ])
}

/// A belt segment leaving with a moving structure: the whole belt leaves its tables the first
/// time (`take_belt`), quietly (no segment breaks into a shaft, nothing drops), its items kept by
/// this segment; each segment is marked as a belt for its drawing.
fn carry_belt(dim: usize, pos: [i32; 3], out: &mut Compound) {
    out.insert(BELT_SEGMENT.into(), Tag::Byte(1));
    let taken = with_world(|w| {
        w.remove_belt_quietly(dim, pos);
        let (controller, belt) = w.take_belt(dim, pos)?;
        for p in &belt.chain {
            w.remove_belt_quietly(dim, *p);
        }
        Some((controller, belt))
    })
    .flatten();
    let Some((controller, belt)) = taken else {
        // A controller whose chain was not whole (a member's column unloaded) still holds its items.
        if let Some(items) = with_world(|w| w.take_staged_belt_items(dim, pos)).flatten() {
            out.insert(
                BELT_ITEMS.into(),
                super::belts_to_nbt(&[([0, 0, 0], items.to_save())]),
            );
        }
        return;
    };
    let offset = [0, 1, 2].map(|i| controller[i] - pos[i]);
    out.insert(
        BELT_ITEMS.into(),
        super::belts_to_nbt(&[(offset, belt.items.to_save())]),
    );
    let shown = create_engine::belt::Contents {
        controller: offset,
        segment: belt.segment,
        length: belt.chain.len() as u16,
        ending: create_engine::belt::Ending::Eject,
        items: belt.items,
    };
    if let Some(bytes) = create_engine::belt::encode(&[shown]).into_iter().next() {
        out.insert(BELT_SHOWN.into(), Tag::ByteArray(bytes));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use create_engine::belt::{Inventory, Item};
    use create_engine::contraption::Transform;
    use create_engine::world::{CreateWorld, StateTable};

    const CONTROLLER: [i32; 3] = [0, 8, 0];
    const END: [i32; 3] = [0, 8, 1];
    const MIDDLE: [i32; 3] = [0, 8, 1];
    const END2: [i32; 3] = [0, 8, 2];

    /// The live whole belt from `CONTROLLER` to `END` (states 10 start, 11 end).
    fn whole_belt(p: [i32; 3]) -> u32 {
        match p {
            CONTROLLER => 10,
            END => 11,
            _ => 0,
        }
    }

    /// States 10 start, 11 end, 12 middle.
    fn belt_landing_at(offset: [i32; 3]) -> (CreateWorld, Compound) {
        let table = StateTable::new(
            10,
            ["start", "end", "middle"].into_iter().map(|part| {
                (
                    "create:belt",
                    [("facing", "south"), ("slope", "horizontal"), ("part", part)].into_iter(),
                )
            }),
        );
        let world = CreateWorld::new(table);
        let data = Compound::from([(
            BELT_ITEMS.into(),
            super::super::belts_to_nbt(&[(
                offset,
                Inventory::fresh(vec![Item::new("minecraft:iron_ingot", 3, 0, false)]),
            )]),
        )]);
        (world, data)
    }

    fn belt_landing() -> (CreateWorld, Compound) {
        belt_landing_at([1, 0, 0])
    }

    /// The items of the belt that starts at `start` (and ends one block south of it) once it has
    /// formed.
    fn finish_belt(world: &mut CreateWorld, start: [i32; 3]) -> Inventory {
        world.block_loaded(0, start, 10);
        world.block_loaded(0, [start[0], start[1], start[2] + 1], 11);
        belt_items_after_tick(world, start)
    }

    fn belt_items_after_tick(world: &mut CreateWorld, start: [i32; 3]) -> Inventory {
        let mut turns = world.transport_ticks(0, 0);
        while turns.next(world).is_some() {}
        let mut belts = world.column_belts(0, start[0] >> 4, start[2] >> 4);
        assert_eq!(belts.len(), 1);
        let (controller, inventory) = belts.pop().unwrap();
        assert_eq!(controller, start);
        inventory
    }

    fn iron(drops: &[([i32; 3], Item)]) -> Vec<([i32; 3], &str, u8)> {
        drops
            .iter()
            .map(|(p, i)| (*p, i.item.as_str(), i.count))
            .collect()
    }

    #[test]
    fn refused_belt_landing_does_not_seed_a_new_belt() {
        let (mut world, data) = belt_landing();
        let turn = Transform::from_place(|[x, y, z]| [z, y, -x]);
        // The landing did not write the existing controller position.
        let drops = restore_belt_items(&mut world, 0, END, &data, turn, |p| p == END, whole_belt);
        assert_eq!(iron(&drops), [(CONTROLLER, "minecraft:iron_ingot", 3)]);
        assert!(finish_belt(&mut world, CONTROLLER).is_empty());
    }

    #[test]
    fn landed_belt_controller_keeps_inventory_when_record_segment_is_refused() {
        let (mut world, data) = belt_landing();
        let turn = Transform::from_place(|[x, y, z]| [z, y, -x]);
        // END already stands in the world as `Part::End` (its write was refused) and joins the
        // newly written CONTROLLER start into a whole chain.
        let drops = restore_belt_items(
            &mut world,
            0,
            END,
            &data,
            turn,
            |p| p == CONTROLLER,
            whole_belt,
        );
        assert!(drops.is_empty());
        let inventory = finish_belt(&mut world, CONTROLLER);
        assert_eq!(inventory.items.len(), 1);
        assert_eq!(inventory.items[0].item, "minecraft:iron_ingot");
        assert_eq!(inventory.items[0].count, 3);
    }

    #[test]
    fn non_belt_at_placed_controller_drops_carried_belt_items() {
        let (mut world, data) = belt_landing();
        let turn = Transform::from_place(|[x, y, z]| [z, y, -x]);
        let drops = restore_belt_items(
            &mut world,
            0,
            END,
            &data,
            turn,
            |p| p == CONTROLLER,
            |p| if p == END { 11 } else { 0 },
        );
        assert_eq!(iron(&drops), [(CONTROLLER, "minecraft:iron_ingot", 3)]);
        assert!(finish_belt(&mut world, CONTROLLER).is_empty());
    }

    #[test]
    fn written_non_controller_belt_segment_drops_carried_belt_items() {
        let (mut world, data) = belt_landing();
        let turn = Transform::from_place(|[x, y, z]| [z, y, -x]);
        let drops = restore_belt_items(
            &mut world,
            0,
            END,
            &data,
            turn,
            |p| p == CONTROLLER,
            |p| if p == CONTROLLER || p == END { 11 } else { 0 },
        );
        assert_eq!(iron(&drops), [(CONTROLLER, "minecraft:iron_ingot", 3)]);
        assert!(world.take_belt_saved_changes(0).is_empty());
        assert!(finish_belt(&mut world, CONTROLLER).is_empty());
    }

    #[test]
    fn partial_belt_landing_without_end_drops_carried_belt_items() {
        let turn = Transform::from_place(|[x, y, z]| [z, y, -x]);
        // Two segments: the start was written, the end was refused and holds air.
        let (mut world, data) = belt_landing();
        let drops = restore_belt_items(
            &mut world,
            0,
            END,
            &data,
            turn,
            |p| p == CONTROLLER,
            |p| if p == CONTROLLER { 10 } else { 0 },
        );
        assert_eq!(iron(&drops), [(CONTROLLER, "minecraft:iron_ingot", 3)]);
        assert!(world.take_belt_saved_changes(0).is_empty());
        assert!(finish_belt(&mut world, CONTROLLER).is_empty());
        // Three segments: start and middle were written, the end was refused.
        let (mut world, data) = belt_landing_at([2, 0, 0]);
        let drops = restore_belt_items(
            &mut world,
            0,
            END2,
            &data,
            turn,
            |p| p == CONTROLLER || p == MIDDLE,
            |p| match p {
                CONTROLLER => 10,
                MIDDLE => 12,
                _ => 0,
            },
        );
        assert_eq!(iron(&drops), [(CONTROLLER, "minecraft:iron_ingot", 3)]);
        assert!(world.take_belt_saved_changes(0).is_empty());
        // The belt that forms there later starts empty.
        world.block_loaded(0, CONTROLLER, 10);
        world.block_loaded(0, MIDDLE, 12);
        world.block_loaded(0, END2, 11);
        assert!(belt_items_after_tick(&mut world, CONTROLLER).is_empty());
    }

    #[test]
    fn overflowing_belt_offset_never_writes_world_and_drops_at_anchor() {
        // A whole live belt stands at `at`, so only the overflow refuses the landing.
        let belt_at = |at: [i32; 3]| {
            move |p: [i32; 3]| {
                if p == at {
                    10
                } else if p == [at[0], at[1], at[2] + 1] {
                    11
                } else {
                    0
                }
            }
        };
        // The sum with the anchor overflows.
        let pos = [i32::MAX, 8, 0];
        let (mut world, data) = belt_landing_at([1, 0, 0]);
        let drops = restore_belt_items(
            &mut world,
            0,
            pos,
            &data,
            Transform::NONE,
            |_| true,
            belt_at(pos),
        );
        assert_eq!(iron(&drops), [(pos, "minecraft:iron_ingot", 3)]);
        // (No belt is formed at this edge: a kinetic part there would look past `i32::MAX`.)
        assert!(world.column_belts(0, pos[0] >> 4, pos[2] >> 4).is_empty());
        assert!(world.take_belt_saved_changes(0).is_empty());
        // The same overflow with a live belt, away from `i32::MAX` for kinetic neighbours.
        let pos = [i32::MAX - 10, 8, 0];
        let (mut world, data) = belt_landing_at([11, 0, 0]);
        let drops = restore_belt_items(
            &mut world,
            0,
            pos,
            &data,
            Transform::NONE,
            |_| true,
            belt_at(pos),
        );
        assert_eq!(iron(&drops), [(pos, "minecraft:iron_ingot", 3)]);
        assert!(world.take_belt_saved_changes(0).is_empty());
        assert!(finish_belt(&mut world, pos).is_empty());
        // The turned offset overflows: `i32::MIN` has no negation.
        let (mut world, data) = belt_landing_at([i32::MIN, 0, 0]);
        let turn = Transform::from_place(|[x, y, z]| [z, y, -x]);
        let drops = restore_belt_items(
            &mut world,
            0,
            CONTROLLER,
            &data,
            turn,
            |_| true,
            belt_at(CONTROLLER),
        );
        assert_eq!(iron(&drops), [(CONTROLLER, "minecraft:iron_ingot", 3)]);
        assert!(world.take_belt_saved_changes(0).is_empty());
        assert!(finish_belt(&mut world, CONTROLLER).is_empty());
    }
}
