//! `BeltConnectorItem.useOn`: sneaking forgets the first shaft; a click on a shaft remembers
//! it, a click on a second shaft that connects lays the belt (one belt connector used up outside
//! creative) and forgets it. Refusals say nothing, as in Create (its client draws the red
//! particles of `BeltConnectorHandler`). Remembering a shaft, or laying a belt with connectors
//! left, starts the item's 5-tick cooldown; a belt laid gives the `belt` advancement.

use super::super::server::{self, ServerLevel, update_all};
use super::super::states;
use super::super::world::with_world;
use super::{creative, custom, held, set_custom, set_held, sneaking};
use create_engine::Block;
use create_engine::belt::Segment;
use create_engine::belt_connector::{self as bc, FIRST_SHAFT_KEY};
use create_engine::kinetics::Axis;
use mod_api::{
    BlockState, ClientHost, Hand, HostBlockStates, HostSide, InteractionResult, ItemPart,
    ItemStack, PlayerRef, ServerHost, SoundSource, Tag, UseOnEvent,
};

pub(super) struct Connector;

#[mod_api::sunrise_part]
impl ItemPart for Connector {
    fn use_on(&self, host: HostSide, event: &mut UseOnEvent<'_>) -> Option<InteractionResult> {
        match host {
            HostSide::Server(host) => use_on(host, event),
            HostSide::Client(host) => predict(host, event),
        }
    }
}

/// `if (world.isClientSide) return validAxis ? SUCCESS : FAIL` (after the sneaking forget).
fn predict(host: &mut dyn ClientHost, event: &UseOnEvent<'_>) -> Option<InteractionResult> {
    let player = host.player(event.player)?;
    if sneaking(&*host, player) {
        return Some(InteractionResult::Success);
    }
    let level = host.level(host.dimension())?;
    let shaft = level
        .block_state(&*host, event.pos)
        .and_then(states::id)
        .and_then(|id| {
            super::super::client_table(
                &*host,
                |t| matches!(t.get(id), Some((Block::Shaft, i)) if i.axis.is_some()),
            )
        })
        .unwrap_or(false);
    Some(if shaft {
        InteractionResult::Success
    } else {
        InteractionResult::Fail
    })
}

/// The world as the belt connector asks it, on the server.
struct Ground<'l, 'a> {
    lv: &'l ServerLevel<'a>,
}

impl bc::Ground for Ground<'_, '_> {
    fn loaded(&self, pos: [i32; 3]) -> bool {
        self.lv.is_loaded(pos)
    }

    fn shaft(&self, pos: [i32; 3]) -> Option<Axis> {
        let state = self.lv.block(pos);
        with_world(|w| match w.table.get(state) {
            Some((Block::Shaft, info)) => info.axis,
            _ => None,
        })
        .flatten()
    }

    fn speed(&self, pos: [i32; 3]) -> f32 {
        let dim = self.lv.dim;
        with_world(|w| w.speed(dim, pos)).flatten().unwrap_or(0.0)
    }

    fn replaceable(&self, pos: [i32; 3]) -> bool {
        self.lv
            .block_state(pos)
            .and_then(|s| self.lv.host.block_states().replaceable(s))
            .unwrap_or(false)
    }
}

/// The belt connector's first shaft (`BELT_FIRST_SHAFT`, a `BlockPos`: three ints).
fn first_shaft(stack: &ItemStack) -> Option<[i32; 3]> {
    match custom(stack).get(&FIRST_SHAFT_KEY.into()) {
        Some(Tag::IntArray(v)) if v.len() == 3 => Some([v[0], v[1], v[2]]),
        _ => None,
    }
}

fn use_on(host: &mut dyn ServerHost, event: &UseOnEvent<'_>) -> Option<InteractionResult> {
    let player = host.player(event.player)?;
    let dimension = player.dimension(&*host)?.to_string();
    let back = sneaking(&*host, player);
    let mut lv = ServerLevel::new(host, &dimension)?;
    let pos = event.pos;
    let first = first_shaft(event.stack);
    let click = bc::click(&Ground { lv: &lv }, back, first, pos, true);
    let hand = event.hand;
    let answer = match click {
        bc::Click::Forget => {
            set_first(lv.host, player, hand, None);
            InteractionResult::Success
        }
        bc::Click::Fail { forget } => {
            if forget {
                set_first(lv.host, player, hand, None);
            }
            InteractionResult::Fail
        }
        bc::Click::Remember => {
            set_first(lv.host, player, hand, Some(pos));
            player.set_item_cooldown(lv.host, bc::ITEM, 5);
            InteractionResult::Success
        }
        bc::Click::Refused => InteractionResult::Success,
        bc::Click::Connect(start) => {
            if !create_belts(&mut lv, start, pos) {
                return Some(InteractionResult::Fail);
            }
            super::super::advancements::award(lv.host, player, "belt");
            if !creative(&*lv.host, player) {
                super::shrink_held(lv.host, player, hand, 1);
            }
            set_first(lv.host, player, hand, None);
            if held(&*lv.host, player, hand).is_some() {
                player.set_item_cooldown(lv.host, bc::ITEM, 5);
            }
            InteractionResult::Success
        }
    };
    Some(answer)
}

fn set_first(host: &mut dyn ServerHost, player: PlayerRef, hand: Hand, pos: Option<[i32; 3]>) {
    if let Some(mut stack) = held(&*host, player, hand) {
        set_custom(
            &mut stack,
            &[(FIRST_SHAFT_KEY, pos.map(|p| Tag::IntArray(p.to_vec())))],
        );
        set_held(host, player, hand, Some(stack));
    }
}

/// Preflights the entire belt before writing. If a write fails, removes placed belts still
/// present and drops the blocks already broken, without restoring their prior states.
#[must_use]
pub(crate) fn create_belts(lv: &mut ServerLevel<'_>, start: [i32; 3], end: [i32; 3]) -> bool {
    let layout = {
        let ground = Ground { lv };
        let Some(axis) = bc::Ground::shaft(&ground, start) else {
            return false;
        };
        bc::layout(start, end, axis, |p| bc::Ground::shaft(&ground, p))
    };
    let Some(belt) = lv.host.block_state_default(states::key(Block::Belt)) else {
        return false;
    };
    let at = bc::sound_at(start, end).map(|v| f64::from(v) + 0.5);
    place_belt_layout(&mut Belts { lv, belt, at }, &layout)
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Cell<S> {
    original: S,
    replaceable: bool,
    target: S,
}

trait BeltWorld {
    type State: Copy + PartialEq;

    fn cell(&self, pos: [i32; 3], seg: Segment) -> Option<Cell<Self::State>>;
    fn destroy(&mut self, pos: [i32; 3]) -> bool;
    fn place_belt(&mut self, pos: [i32; 3], state: Self::State) -> bool;
    fn is_belt(&self, pos: [i32; 3]) -> bool;
    fn drop_block(&mut self, pos: [i32; 3], state: Self::State);
    fn sound(&mut self);
}

struct Belts<'l, 'a> {
    lv: &'l mut ServerLevel<'a>,
    belt: BlockState,
    at: [f64; 3],
}

impl BeltWorld for Belts<'_, '_> {
    type State = BlockState;

    fn cell(&self, pos: [i32; 3], seg: Segment) -> Option<Cell<BlockState>> {
        let state = self.lv.block_state(pos)?;
        let facts = self.lv.host.block_states();
        if facts.destroy_speed(state).is_some_and(|h| h < 0.0) {
            return None;
        }
        let replaceable = facts.replaceable(state).unwrap_or(false);
        let is_shaft = state.block() == states::key(Block::Shaft);
        if !replaceable && !is_shaft && self.lv.host.block_entity(self.lv.level, pos).is_some() {
            return None;
        }
        let waterlogged = server::is_water_source(&*self.lv.host, state);
        let mut target = self.belt;
        for (k, v) in [
            ("facing", super::super::kinetics::dir_name(seg.facing)),
            ("slope", bc::slope_name(seg.slope)),
            ("part", bc::part_name(seg.part)),
            ("waterlogged", if waterlogged { "true" } else { "false" }),
        ] {
            target = target.with(&facts, k, v).ok()?;
        }
        Some(Cell {
            original: state,
            replaceable,
            target,
        })
    }

    fn destroy(&mut self, pos: [i32; 3]) -> bool {
        self.lv.destroy_block(pos, false)
    }

    fn place_belt(&mut self, pos: [i32; 3], state: BlockState) -> bool {
        self.lv.set_state(pos, state, update_all())
    }

    fn is_belt(&self, pos: [i32; 3]) -> bool {
        self.lv
            .block_state(pos)
            .is_some_and(|s| s.block() == states::key(Block::Belt))
    }

    fn drop_block(&mut self, pos: [i32; 3], state: BlockState) {
        self.lv.drop_block(pos, state);
    }

    fn sound(&mut self) {
        self.lv.level.play_sound(
            self.lv.host,
            "minecraft:block.wool.place",
            self.at,
            SoundSource::Blocks,
            0.5,
            1.0,
            None,
        );
    }
}

fn clean_up<W: BeltWorld>(world: &mut W, placed: Vec<[i32; 3]>, broken: Vec<([i32; 3], W::State)>) {
    for pos in placed {
        // A neighbour update may have replaced this belt (BeltConnectorItem.java:144).
        if world.is_belt(pos) {
            world.destroy(pos);
        }
    }
    for (pos, original) in broken {
        world.drop_block(pos, original);
    }
}

fn place_belt_layout<W: BeltWorld>(world: &mut W, layout: &[([i32; 3], Segment)]) -> bool {
    if layout.is_empty() {
        return false;
    }
    let Some(cells) = layout
        .iter()
        .map(|&(pos, seg)| world.cell(pos, seg))
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };
    let mut placed = Vec::new();
    let mut broken = Vec::new();
    for (index, (&(pos, seg), cell)) in layout.iter().zip(cells).enumerate() {
        // Earlier writes may have changed this cell (`update_all`): never break or place over
        // what the preflight did not see.
        if index > 0 && world.cell(pos, seg).as_ref() != Some(&cell) {
            clean_up(world, placed, broken);
            return false;
        }
        if !cell.replaceable {
            if !world.destroy(pos) {
                clean_up(world, placed, broken);
                return false;
            }
            broken.push((pos, cell.original));
        }
        if !world.place_belt(pos, cell.target) {
            clean_up(world, placed, broken);
            return false;
        }
        placed.push(pos);
    }
    world.sound();
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use create_engine::belt::{Part, Slope};
    use create_engine::kinetics::Dir;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum State {
        Air,
        Shaft(Axis),
        Belt(Part),
        Stone,
    }

    use State::{Air, Belt, Shaft, Stone};

    #[derive(Debug, PartialEq, Eq)]
    enum Change {
        Destroy([i32; 3]),
        Place([i32; 3], State),
        Drop([i32; 3], State),
        Sound,
    }

    use Change::{Destroy, Drop, Place, Sound};

    const A: [i32; 3] = [0, 0, 0];
    const B: [i32; 3] = [1, 0, 0];
    const C: [i32; 3] = [2, 0, 0];
    const D: [i32; 3] = [3, 0, 0];
    const LAYOUT: [([i32; 3], Segment); 4] = [
        (A, segment(Part::Start)),
        (B, segment(Part::Middle)),
        (C, segment(Part::Pulley)),
        (D, segment(Part::End)),
    ];
    const ORIGINAL: [State; 4] = [Shaft(Axis::Z), Air, Shaft(Axis::Z), Shaft(Axis::Z)];

    const fn segment(part: Part) -> Segment {
        Segment {
            facing: Dir::East,
            slope: Slope::Horizontal,
            part,
        }
    }

    struct Fake {
        states: [State; 4],
        closed: Option<[i32; 3]>,
        refuse_break: Option<[i32; 3]>,
        refuse_place: Option<[i32; 3]>,
        replace_on_refusal: Option<[i32; 3]>,
        replace_after_placing_first: Option<[i32; 3]>,
        log: Vec<Change>,
    }

    impl Default for Fake {
        fn default() -> Self {
            Self {
                states: ORIGINAL,
                closed: None,
                refuse_break: None,
                refuse_place: None,
                replace_on_refusal: None,
                replace_after_placing_first: None,
                log: Vec::new(),
            }
        }
    }

    impl BeltWorld for Fake {
        type State = State;

        fn cell(&self, pos: [i32; 3], seg: Segment) -> Option<Cell<State>> {
            let original = self.states[pos[0] as usize];
            (self.closed != Some(pos)).then_some(Cell {
                original,
                replaceable: original == Air,
                target: Belt(seg.part),
            })
        }

        fn destroy(&mut self, pos: [i32; 3]) -> bool {
            self.log.push(Destroy(pos));
            if self.refuse_break == Some(pos) {
                return false;
            }
            self.states[pos[0] as usize] = Air;
            true
        }

        fn place_belt(&mut self, pos: [i32; 3], state: State) -> bool {
            self.log.push(Place(pos, state));
            if self.refuse_place == Some(pos) {
                if let Some(replaced) = self.replace_on_refusal {
                    self.states[replaced[0] as usize] = Stone;
                }
                return false;
            }
            self.states[pos[0] as usize] = state;
            if pos == A
                && let Some(replaced) = self.replace_after_placing_first
            {
                self.states[replaced[0] as usize] = Stone;
            }
            true
        }

        fn is_belt(&self, pos: [i32; 3]) -> bool {
            matches!(self.states[pos[0] as usize], Belt(_))
        }

        fn drop_block(&mut self, pos: [i32; 3], state: State) {
            self.log.push(Drop(pos, state));
        }

        fn sound(&mut self) {
            self.log.push(Sound);
        }
    }

    #[test]
    fn a_closed_cell_stops_before_any_block_changes() {
        for layout in [&LAYOUT[..], &[]] {
            let mut world = Fake {
                closed: Some(C),
                ..Fake::default()
            };
            assert!(!place_belt_layout(&mut world, layout));
            assert_eq!(world.states, ORIGINAL);
            assert!(world.log.is_empty());
        }
    }

    #[test]
    fn a_refused_break_removes_placed_belts_and_drops_broken_blocks() {
        let mut world = Fake {
            refuse_break: Some(C),
            ..Fake::default()
        };
        assert!(!place_belt_layout(&mut world, &LAYOUT));
        assert_eq!(world.states, [Air, Air, ORIGINAL[2], ORIGINAL[3]]);
        assert_eq!(
            world.log,
            [
                Destroy(A),
                Place(A, Belt(Part::Start)),
                Place(B, Belt(Part::Middle)),
                Destroy(C),
                Destroy(A),
                Destroy(B),
                Drop(A, ORIGINAL[0]),
            ]
        );
    }

    #[test]
    fn a_refused_placement_after_breaking_a_shaft_drops_broken_shafts() {
        let mut world = Fake {
            refuse_place: Some(C),
            ..Fake::default()
        };
        assert!(!place_belt_layout(&mut world, &LAYOUT));
        assert_eq!(world.states, [Air, Air, Air, ORIGINAL[3]]);
        assert_eq!(
            world.log,
            [
                Destroy(A),
                Place(A, Belt(Part::Start)),
                Place(B, Belt(Part::Middle)),
                Destroy(C),
                Place(C, Belt(Part::Pulley)),
                Destroy(A),
                Destroy(B),
                Drop(A, ORIGINAL[0]),
                Drop(C, ORIGINAL[2]),
            ]
        );
    }

    #[test]
    fn cleanup_skips_placed_cells_no_longer_holding_a_belt() {
        let mut world = Fake {
            refuse_place: Some(C),
            replace_on_refusal: Some(A),
            ..Fake::default()
        };
        assert!(!place_belt_layout(&mut world, &LAYOUT));
        assert_eq!(world.states, [Stone, Air, Air, ORIGINAL[3]]);
        assert_eq!(
            world.log,
            [
                Destroy(A),
                Place(A, Belt(Part::Start)),
                Place(B, Belt(Part::Middle)),
                Destroy(C),
                Place(C, Belt(Part::Pulley)),
                Destroy(B),
                Drop(A, ORIGINAL[0]),
                Drop(C, ORIGINAL[2]),
            ]
        );
    }

    #[test]
    fn a_changed_later_cell_aborts_before_writing_it() {
        for (changed, states, log) in [
            (
                B,
                [Air, Stone, ORIGINAL[2], ORIGINAL[3]],
                vec![
                    Destroy(A),
                    Place(A, Belt(Part::Start)),
                    Destroy(A),
                    Drop(A, ORIGINAL[0]),
                ],
            ),
            (
                C,
                [Air, Air, Stone, ORIGINAL[3]],
                vec![
                    Destroy(A),
                    Place(A, Belt(Part::Start)),
                    Place(B, Belt(Part::Middle)),
                    Destroy(A),
                    Destroy(B),
                    Drop(A, ORIGINAL[0]),
                ],
            ),
        ] {
            let mut world = Fake {
                replace_after_placing_first: Some(changed),
                ..Fake::default()
            };
            assert!(!place_belt_layout(&mut world, &LAYOUT));
            assert_eq!(world.states, states);
            assert_eq!(world.log, log);
        }
    }

    #[test]
    fn a_laid_belt_sounds_once() {
        let mut world = Fake::default();
        assert!(place_belt_layout(&mut world, &LAYOUT));
        assert_eq!(world.states, LAYOUT.map(|(_, seg)| Belt(seg.part)));
        assert_eq!(
            world.log,
            [
                Destroy(A),
                Place(A, Belt(Part::Start)),
                Place(B, Belt(Part::Middle)),
                Destroy(C),
                Place(C, Belt(Part::Pulley)),
                Destroy(D),
                Place(D, Belt(Part::End)),
                Sound,
            ]
        );
    }
}
