//! Sequenced gearshifts (`SequencedGearshiftBlockEntity`: `getRotationSpeedModifier`, `run`,
//! `onRedstoneUpdate`) in the networks `RotationPropagator` builds around them. Four histories,
//! picked by the seed's index, each on the four ways to stand the block (both horizontal axes
//! and the vertical shaft in either `axis` value):
//! - `between`: two generators facing it, with and without shafts on either side, placed in
//!   every order, with the block idle (state 0) or running each instruction kind;
//! - `dead_face`: the far side turned (a hand crank or a wrench on its shaft) while the program
//!   runs, then either side's source removed;
//! - `gearbox_loop`: a ring of gearboxes that leads the far side back into the block's own
//!   network, then cut;
//! - `clutch_flicker`: a clutch on either face (or both) toggled by a signal every tick, so the
//!   speed crosses zero until `KineticBlockEntity`'s flicker tally breaks the block.
use crate::actions::{Action, Face};
use crate::program::{Instruction, Kind, Modifier};
use crate::scenario::{Change, Placed, Pos, Scenario};

const BASE: Pos = [0, 8, 0];
/// Local steps: along the sequencer's axis, across it, and normal to both.
const U: Pos = [1, 0, 0];
const V: Pos = [0, 1, 0];
const W: Pos = [0, 0, 1];
const BACK: Pos = [-1, 0, 0];

/// One orientation of a layout: where local `u`, `v` and `w` point in the world.
struct Frame {
    basis: [Pos; 3],
    /// The sequencer's `axis` property.
    axis: &'static str,
    vertical: bool,
}

impl Frame {
    fn new(orientation: u64) -> Frame {
        let (basis, axis, vertical) = match orientation {
            0 => ([[1, 0, 0], [0, 1, 0], [0, 0, 1]], "x", false),
            1 => ([[0, 0, 1], [0, 1, 0], [1, 0, 0]], "z", false),
            2 => ([[0, 1, 0], [1, 0, 0], [0, 0, 1]], "x", true),
            _ => ([[0, 1, 0], [0, 0, 1], [1, 0, 0]], "z", true),
        };
        Frame {
            basis,
            axis,
            vertical,
        }
    }
    /// The world step of a local one.
    fn step(&self, local: Pos) -> Pos {
        std::array::from_fn(|a| local.iter().zip(&self.basis).map(|(n, b)| n * b[a]).sum())
    }
    fn at(&self, local: Pos) -> Pos {
        let step = self.step(local);
        [BASE[0] + step[0], BASE[1] + step[1], BASE[2] + step[2]]
    }
    fn face(&self, local: Pos) -> Face {
        match self.step(local) {
            [1, 0, 0] => Face::East,
            [-1, 0, 0] => Face::West,
            [0, 1, 0] => Face::Up,
            [0, -1, 0] => Face::Down,
            [0, 0, 1] => Face::South,
            _ => Face::North,
        }
    }
    fn facing(&self, local: Pos) -> &'static str {
        match self.face(local) {
            Face::East => "east",
            Face::West => "west",
            Face::Up => "up",
            Face::Down => "down",
            Face::South => "south",
            Face::North => "north",
        }
    }
    fn axis_of(&self, local: Pos) -> &'static str {
        let step = self.step(local);
        if step[0] != 0 {
            "x"
        } else if step[1] != 0 {
            "y"
        } else {
            "z"
        }
    }
    fn block(&self, local: Pos, state: String) -> Placed {
        Placed {
            pos: self.at(local),
            state,
            motor: None,
        }
    }
    fn sequencer(&self) -> Placed {
        self.block(
            [0, 0, 0],
            format!(
                "create:sequenced_gearshift[axis={},state=0,vertical={}]",
                self.axis, self.vertical
            ),
        )
    }
    fn shaft(&self, local: Pos, along: Pos) -> Placed {
        self.block(
            local,
            format!(
                "create:shaft[axis={},waterlogged=false]",
                self.axis_of(along)
            ),
        )
    }
    fn clutch(&self, local: Pos) -> Placed {
        self.block(
            local,
            format!("create:clutch[axis={},powered=false]", self.axis_of(U)),
        )
    }
    fn gearbox(&self, local: Pos) -> Placed {
        self.block(local, format!("create:gearbox[axis={}]", self.axis_of(W)))
    }
    fn crank(&self, local: Pos, facing: Pos) -> Placed {
        self.block(
            local,
            format!(
                "create:hand_crank[facing={},waterlogged=false]",
                self.facing(facing)
            ),
        )
    }
    fn motor(&self, local: Pos, facing: Pos, rpm: i32) -> Placed {
        Placed {
            motor: Some(rpm),
            ..self.block(
                local,
                format!("create:creative_motor[facing={}]", self.facing(facing)),
            )
        }
    }
}

fn event(tick: u32, pos: Pos, action: Action) -> Change {
    Change {
        tick,
        pos,
        state: None,
        motor: None,
        turn: None,
        action: Some(action),
    }
}

fn signal(tick: u32, pos: Pos, strength: u8) -> Change {
    event(tick, pos, Action::Signal { strength })
}

fn scenario(
    seed: u64,
    ticks: u32,
    checkpoints: Vec<u32>,
    blocks: Vec<Placed>,
    events: Vec<Change>,
) -> Scenario {
    Scenario {
        version: 3,
        seed,
        day_time: 0,
        ticks,
        checkpoints,
        blocks,
        events,
    }
}

/// The next of `n` choices, most frequent first.
fn digit(k: &mut u64, n: u64) -> u64 {
    let d = *k % n;
    *k /= n;
    d
}

/// The blocks in an order taken from `k`: every order of the first five is reached.
fn shuffled(mut parts: Vec<Placed>, k: &mut u64) -> Vec<Placed> {
    let mut order = digit(k, (1..=parts.len().min(5) as u64).product());
    let mut out = Vec::new();
    while !parts.is_empty() {
        let n = parts.len() as u64;
        out.push(parts.remove((order % n) as usize));
        order /= n;
    }
    out
}

/// The program a player sends for run `kind`: 0 sends none (the block stays idle at state 0),
/// 1 to 5 lead with `TURN_ANGLE`, `TURN_DISTANCE`, `DELAY`, `AWAIT` and `END`.
fn program(kind: u64, modifier: u64) -> Option<Vec<Instruction>> {
    let modifier = [
        Modifier::ForwardFast,
        Modifier::Forward,
        Modifier::Back,
        Modifier::BackFast,
    ][modifier as usize];
    let step = |instruction, modifier, value| Instruction {
        instruction,
        modifier,
        value,
    };
    let end = step(Kind::End, Modifier::Forward, -1);
    Some(match kind {
        0 => return None,
        1 => vec![step(Kind::TurnAngle, modifier, 90), end],
        2 => vec![step(Kind::TurnDistance, modifier, 2), end],
        3 => vec![
            step(Kind::Delay, Modifier::Forward, 4),
            step(Kind::TurnAngle, modifier, 90),
            end,
        ],
        4 => vec![
            step(Kind::Await, Modifier::Forward, -1),
            step(Kind::TurnAngle, modifier, 90),
            end,
        ],
        _ => vec![end],
    })
}

pub fn generate(seed: u64, index: u64) -> Scenario {
    let k = index / 4;
    match index % 4 {
        0 => between(seed, k),
        1 => dead_face(seed, k),
        2 => gearbox_loop(seed, k),
        _ => clutch_flicker(seed, k),
    }
}

/// Two motors facing the block from opposite sides, with a shaft or none beside it on either
/// side: the program arrives, then the signal runs, resumes (`AWAIT`) and stops it.
fn between(seed: u64, mut k: u64) -> Scenario {
    let frame = Frame::new(digit(&mut k, 4));
    let kind = digit(&mut k, 6);
    let shaft_left = digit(&mut k, 2) == 1;
    let shaft_right = digit(&mut k, 2) == 1;
    let mut parts = vec![
        frame.sequencer(),
        frame.motor([-1 - i32::from(shaft_left), 0, 0], U, 32),
    ];
    if shaft_left {
        parts.push(frame.shaft([-1, 0, 0], U));
    }
    if shaft_right {
        parts.push(frame.shaft([1, 0, 0], U));
    }
    let right_rpm = [32, -32, 64, -16][digit(&mut k, 4) as usize];
    parts.push(frame.motor([1 + i32::from(shaft_right), 0, 0], BACK, right_rpm));
    let blocks = shuffled(parts, &mut k);
    let mut events = Vec::new();
    if let Some(instructions) = program(kind, digit(&mut k, 4)) {
        events.push(event(12, BASE, Action::Program { instructions }));
    }
    for (tick, strength) in [(20, 15), (24, 0), (40, 15), (44, 0), (70, 15), (74, 0)] {
        events.push(signal(tick, frame.at(V), strength));
    }
    scenario(seed, 130, (19..100).collect(), blocks, events)
}

/// A motor behind the block and shafts on its far side, ending in a hand crank or a second
/// motor. While the program runs the far side is turned: the crank (alternating directions) or
/// a wrench on the shaft beside the block. Then either source is removed, the other later.
fn dead_face(seed: u64, mut k: u64) -> Scenario {
    let frame = Frame::new(digit(&mut k, 4));
    let crank = digit(&mut k, 2) == 0;
    let near_first = digit(&mut k, 2) == 0;
    let kind = 1 + digit(&mut k, 5);
    let instructions = program(kind, digit(&mut k, 4)).unwrap();
    let far = [3, 0, 0];
    let mut blocks = shuffled(
        vec![
            frame.sequencer(),
            frame.motor(BACK, U, 32),
            frame.shaft([1, 0, 0], U),
            frame.shaft([2, 0, 0], U),
        ],
        &mut k,
    );
    // A crank stands against its support, so it goes in last.
    blocks.push(if crank {
        frame.crank(far, U)
    } else {
        frame.motor(far, BACK, 32)
    });
    let mut events = vec![event(12, BASE, Action::Program { instructions })];
    for (tick, strength) in [(20, 15), (24, 0), (50, 15), (54, 0), (110, 15), (114, 0)] {
        events.push(signal(tick, frame.at(V), strength));
    }
    for (i, tick) in (22..60).step_by(4).enumerate() {
        events.push(if crank {
            Change::turn(tick, frame.at(far), i % 3 == 2)
        } else {
            let face = frame.face(V);
            event(
                tick,
                frame.at([1, 0, 0]),
                Action::Use {
                    item: "create:wrench".into(),
                    face,
                    hit: face.point(0.5, 0.5),
                    yaw: 0.,
                    pitch: 0.,
                    sneaking: false,
                },
            )
        });
    }
    let (first, second) = if near_first {
        (frame.at(BACK), frame.at(far))
    } else {
        (frame.at(far), frame.at(BACK))
    };
    events.push(event(70, first, Action::Remove));
    events.push(event(100, second, Action::Remove));
    scenario(seed, 150, (19..120).collect(), blocks, events)
}

/// The block between two gearboxes of a ring (shafts up each side, gearboxes at the top
/// corners, a shaft across): the far side is also reached around the ring, so it stays in the
/// block's network however the speed modifier changes. A motor drives one lower gearbox;
/// afterwards the ring is cut at the top shaft or a top gearbox, and the motor removed.
fn gearbox_loop(seed: u64, mut k: u64) -> Scenario {
    let frame = Frame::new(digit(&mut k, 4));
    let cut = digit(&mut k, 3);
    let motor_right = digit(&mut k, 2) == 1;
    let kind = digit(&mut k, 6);
    let modifier = digit(&mut k, 4);
    let mut parts = vec![frame.sequencer()];
    for local in [[-1, 0, 0], [1, 0, 0], [-1, 2, 0], [1, 2, 0]] {
        parts.push(frame.gearbox(local));
    }
    for local in [[-1, 1, 0], [1, 1, 0]] {
        parts.push(frame.shaft(local, V));
    }
    parts.push(frame.shaft([0, 2, 0], U));
    let motor = [if motor_right { 1 } else { -1 }, -1, 0];
    parts.push(frame.motor(motor, V, 32));
    let blocks = shuffled(parts, &mut k);
    let mut events = Vec::new();
    if let Some(instructions) = program(kind, modifier) {
        events.push(event(12, BASE, Action::Program { instructions }));
    }
    for (tick, strength) in [(20, 15), (24, 0), (40, 15), (44, 0), (90, 15), (94, 0)] {
        events.push(signal(tick, frame.at(V), strength));
    }
    match cut {
        1 => events.push(event(60, frame.at([0, 2, 0]), Action::Remove)),
        2 => events.push(event(60, frame.at([-1, 2, 0]), Action::Remove)),
        _ => {}
    }
    events.push(event(80, frame.at(motor), Action::Remove));
    scenario(seed, 130, (19..110).collect(), blocks, events)
}

/// A motor drives the block through a clutch on its near face, its far face, or one on each
/// (the far one beside a second motor), each powered on alternate ticks: the speed through
/// the block crosses zero every tick (`KineticBlockEntity.onSpeedChanged` adds five to the
/// flicker tally, `tick` takes one) from either face, with the program idle or running.
fn clutch_flicker(seed: u64, mut k: u64) -> Scenario {
    let frame = Frame::new(digit(&mut k, 4));
    let side = digit(&mut k, 3);
    let phase = digit(&mut k, 2) as u32;
    let kind = digit(&mut k, 6);
    let modifier = digit(&mut k, 4);
    let (near, far) = (side != 1, side != 0);
    let mut parts = vec![frame.sequencer()];
    let mut clutches = Vec::new();
    if near {
        parts.push(frame.motor([-2, 0, 0], U, 32));
        parts.push(frame.clutch([-1, 0, 0]));
        clutches.push([-1, 0, 0]);
    } else {
        parts.push(frame.motor(BACK, U, 32));
    }
    if far {
        parts.push(frame.clutch([1, 0, 0]));
        clutches.push([1, 0, 0]);
        parts.push(if near {
            frame.motor([2, 0, 0], BACK, 32)
        } else {
            frame.shaft([2, 0, 0], U)
        });
    } else {
        parts.push(frame.shaft([1, 0, 0], U));
    }
    let blocks = shuffled(parts, &mut k);
    let mut events = Vec::new();
    if let Some(instructions) = program(kind, modifier) {
        events.push(event(12, BASE, Action::Program { instructions }));
    }
    events.push(signal(18, frame.at(V), 15));
    events.push(signal(104, frame.at(V), 0));
    for tick in 20..100 {
        for (i, clutch) in clutches.iter().enumerate() {
            let on = (tick + phase + i as u32) % 2 == 0;
            events.push(signal(
                tick,
                frame.at([clutch[0], 1, 0]),
                if on { 15 } else { 0 },
            ));
        }
    }
    scenario(seed, 180, (19..105).collect(), blocks, events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::parse_state;
    use std::collections::BTreeSet;

    fn shape(shape: u64, k: u64) -> Scenario {
        generate(0, shape + 4 * k)
    }
    fn count(sc: &Scenario, id: &str) -> usize {
        sc.blocks
            .iter()
            .filter(|b| parse_state(&b.state).0 == id)
            .count()
    }
    fn facing_step(state: &str) -> Pos {
        let (_, props) = parse_state(state);
        match props
            .iter()
            .find(|(k, _)| k == "facing")
            .unwrap()
            .1
            .as_str()
        {
            "east" => [1, 0, 0],
            "west" => [-1, 0, 0],
            "up" => [0, 1, 0],
            "down" => [0, -1, 0],
            "south" => [0, 0, 1],
            _ => [0, 0, -1],
        }
    }
    fn signals(sc: &Scenario, pos: Pos) -> Vec<(u32, u8)> {
        let mut out: Vec<_> = sc
            .events
            .iter()
            .filter(|e| e.pos == pos)
            .filter_map(|e| match e.action {
                Some(Action::Signal { strength }) => Some((e.tick, strength)),
                _ => None,
            })
            .collect();
        out.sort();
        out
    }
    fn sum(p: Pos) -> i32 {
        p[0] + p[1] + p[2]
    }

    #[test]
    fn every_shape_is_a_valid_scenario() {
        for index in 0..800 {
            let sc = generate(0, index);
            crate::catalogue::scenario(&sc);
            for b in &sc.blocks {
                crate::catalogue::validate(&b.state).unwrap();
            }
            assert_eq!(count(&sc, "create:sequenced_gearshift"), 1, "index {index}");
        }
    }

    #[test]
    fn programs_run_every_instruction_kind_or_none() {
        let mut kinds = Vec::new();
        let mut idle = false;
        for k in 0..96 {
            let sc = shape(0, k);
            let program = sc.events.iter().find_map(|e| match &e.action {
                Some(Action::Program { instructions }) => Some(instructions),
                _ => None,
            });
            idle |= program.is_none();
            for i in program.into_iter().flatten() {
                if !kinds.contains(&i.instruction) {
                    kinds.push(i.instruction);
                }
            }
        }
        assert!(idle);
        for kind in [
            Kind::TurnAngle,
            Kind::TurnDistance,
            Kind::Delay,
            Kind::Await,
            Kind::End,
        ] {
            assert!(kinds.contains(&kind), "{kind:?}");
        }
    }

    #[test]
    fn two_generators_face_the_block_from_opposite_sides_on_every_axis() {
        let (mut states, mut facings) = (BTreeSet::new(), BTreeSet::new());
        for k in 0..64 {
            let sc = shape(0, k);
            let motors: Vec<_> = sc.blocks.iter().filter(|b| b.motor.is_some()).collect();
            assert_eq!(motors.len(), 2);
            let steps: Vec<Pos> = motors.iter().map(|m| facing_step(&m.state)).collect();
            assert_eq!(
                [0, 1, 2].map(|a| steps[0][a] + steps[1][a]),
                [0, 0, 0],
                "k {k}"
            );
            for (m, step) in motors.iter().zip(steps) {
                assert!(
                    (1..=2).any(|n| (0..3).all(|a| m.pos[a] + n * step[a] == BASE[a])),
                    "k {k}: {} does not face the block",
                    m.state
                );
                facings.insert(m.state.clone());
            }
            let sequencer = sc.blocks.iter().find(|b| b.pos == BASE).unwrap();
            states.insert(sequencer.state.clone());
        }
        assert_eq!(states.len(), 4);
        for facing in ["east", "west", "north", "south", "up", "down"] {
            assert!(
                facings
                    .iter()
                    .any(|s| s.contains(&format!("facing={facing}"))),
                "{facing}"
            );
        }
    }

    #[test]
    fn shafts_and_placement_orders_vary() {
        let mut orders = BTreeSet::new();
        let mut shafts = BTreeSet::new();
        for k in (0..4 * 6 * 4 * 4 * 120).step_by(4) {
            let sc = shape(0, k);
            orders.insert(sc.blocks.iter().map(|b| b.pos).collect::<Vec<_>>());
            shafts.insert(count(&sc, "create:shaft"));
        }
        assert!(orders.len() >= 120, "{}", orders.len());
        assert_eq!(shafts, BTreeSet::from([0, 1, 2]));
    }

    #[test]
    fn the_dead_face_is_turned_while_the_program_runs_then_the_sources_go() {
        let (mut cranks, mut wrenches) = (false, false);
        for k in 0..96 {
            let sc = shape(1, k);
            let turns: Vec<_> = sc
                .events
                .iter()
                .filter(|e| match &e.action {
                    Some(Action::Use { item, .. }) => {
                        wrenches = true;
                        item == "create:wrench"
                    }
                    _ => {
                        cranks |= e.turn.is_some();
                        e.turn.is_some()
                    }
                })
                .map(|e| e.tick)
                .collect();
            assert!(!turns.is_empty());
            assert!(turns.iter().all(|t| (22..60).contains(t)), "k {k}");
            assert!(
                sc.events
                    .iter()
                    .any(|e| matches!(e.action, Some(Action::Program { .. })) && e.tick < 20)
            );
            let removed: Vec<_> = sc
                .events
                .iter()
                .filter(|e| matches!(e.action, Some(Action::Remove)))
                .collect();
            assert_eq!(removed.len(), 2);
            assert_ne!(removed[0].pos, removed[1].pos);
            assert!(removed.iter().all(|e| e.tick > 60));
        }
        assert!(cranks && wrenches);
    }

    #[test]
    fn the_gearbox_loop_joins_the_far_side_back_to_the_block() {
        let mut cuts = BTreeSet::new();
        for k in 0..144 {
            let sc = shape(2, k);
            let gearboxes: Vec<_> = sc
                .blocks
                .iter()
                .filter(|b| parse_state(&b.state).0 == "create:gearbox")
                .map(|b| b.pos)
                .collect();
            assert_eq!(gearboxes.len(), 4);
            let beside = |p: &Pos| (0..3).map(|a| (p[a] - BASE[a]).abs()).sum::<i32>() == 1;
            assert_eq!(gearboxes.iter().filter(|p| beside(p)).count(), 2, "k {k}");
            assert_eq!(count(&sc, "create:shaft"), 3);
            assert_eq!(count(&sc, "create:creative_motor"), 1);
            cuts.insert(
                sc.events
                    .iter()
                    .filter(|e| matches!(e.action, Some(Action::Remove)))
                    .count(),
            );
        }
        assert_eq!(cuts, BTreeSet::from([1, 2]));
    }

    #[test]
    fn a_clutch_toggled_every_tick_sits_on_either_face_or_both() {
        let mut layouts = BTreeSet::new();
        for k in 0..96 {
            let sc = shape(3, k);
            let frame = Frame::new(k % 4);
            let clutches: Vec<_> = sc
                .blocks
                .iter()
                .filter(|b| parse_state(&b.state).0 == "create:clutch")
                .map(|b| b.pos)
                .collect();
            let mut layout = Vec::new();
            let mut previous: Vec<Vec<u8>> = Vec::new();
            for clutch in &clutches {
                layout.push(sum(*clutch) < sum(BASE));
                let step = frame.step(V);
                let above = [
                    clutch[0] + step[0],
                    clutch[1] + step[1],
                    clutch[2] + step[2],
                ];
                let toggles = signals(&sc, above);
                assert_eq!(toggles.len(), 80, "k {k}");
                assert!(toggles.windows(2).all(|w| w[0].1 != w[1].1));
                assert_eq!(toggles[0].0, 20);
                previous.push(toggles.iter().map(|t| t.1).collect());
            }
            layout.sort();
            if let [a, b] = &previous[..] {
                assert!(a.iter().zip(b).all(|(x, y)| x != y), "k {k}");
            }
            layouts.insert(layout);
        }
        assert_eq!(
            layouts,
            BTreeSet::from([vec![false], vec![true], vec![false, true]])
        );
    }

    #[test]
    fn the_engine_runs_every_shape_without_panicking() {
        for shape_index in 0..4 {
            for k in [0, 5, 17, 38] {
                let sc = shape(shape_index, k);
                let snap = crate::engine::run(&sc);
                for tick in &sc.checkpoints {
                    assert!(
                        snap.checkpoints.contains_key(tick),
                        "shape {shape_index} k {k}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_family_seeds_reach_every_shape_and_reload() {
        for i in 0..8 {
            let seed = 173 + 256 * i;
            assert_eq!(
                crate::generate::family_name(seed),
                "transmission/sequenced_gearshift"
            );
            let sc = crate::generate::generate(seed);
            crate::catalogue::scenario(&sc);
            crate::engine::run(&sc);
        }
    }
}
