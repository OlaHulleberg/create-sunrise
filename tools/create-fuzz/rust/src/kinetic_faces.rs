//! Player placement and wrench turns against every face of the kinetic blocks a player builds
//! with. Each case lays out one cell per face, so the six clicked faces of a target share a case.

use crate::actions::{Action, Face};
use crate::scenario::{Change, Placed, Pos, Scenario};

const FACES: [Face; 6] = [
    Face::Up,
    Face::Down,
    Face::North,
    Face::South,
    Face::West,
    Face::East,
];

/// Every kinetic block item `contested_placement` places, and the flywheel.
const ITEMS: [&str; 16] = [
    "create:shaft",
    "create:cogwheel",
    "create:large_cogwheel",
    "create:gearbox",
    "create:vertical_gearbox",
    "create:clutch",
    "create:gearshift",
    "create:encased_chain_drive",
    "create:adjustable_chain_gearshift",
    "create:sequenced_gearshift",
    "create:rotation_speed_controller",
    "create:creative_motor",
    "create:hand_crank",
    "create:speedometer",
    "create:stressometer",
    "create:flywheel",
];

/// Clicked kinetic blocks: shaft ends and sides (`ShaftBlock.useItemOn`'s placement helper,
/// `GaugeBlock.getStateForPlacement` on a shaft end), cog faces (`CogwheelBlockItem`'s
/// helpers, `CogWheelBlock.getAxisForPlacement`), a gearbox and an encased shaft
/// (`hasShaftTowards` on all six faces).
const TARGETS: [&str; 6] = [
    "create:shaft[axis=x]",
    "create:shaft[axis=y]",
    "create:cogwheel[axis=z]",
    "create:large_cogwheel[axis=y]",
    "create:gearbox[axis=x]",
    "create:andesite_encased_shaft[axis=z]",
];

fn act(tick: u32, pos: Pos, action: Action) -> Change {
    Change {
        tick,
        pos,
        action: Some(action),
        state: None,
        motor: None,
        turn: None,
    }
}

fn click(tick: u32, pos: Pos, item: &str, face: Face, look: (f32, f32), sneaking: bool) -> Change {
    act(
        tick,
        pos,
        Action::Use {
            item: item.into(),
            face,
            hit: face.point(0.5, 0.5),
            yaw: look.0,
            pitch: look.1,
            sneaking,
        },
    )
}

fn look(k: u64) -> (f32, f32) {
    (
        [10., 100., 190., 280.][(k % 4) as usize],
        [0., 70., -70.][(k / 4 % 3) as usize],
    )
}

/// Use each block item on one face of a kinetic block: six cells, one per clicked face, each
/// with another item. Over 192 consecutive indices every item meets every face of every
/// target, sneaking and not.
pub fn face_placement(seed: u64, index: u64) -> Scenario {
    let sneaking = index % 2 == 1;
    let target = TARGETS[(index / 2 % 6) as usize];
    let offset = index / 12;
    let mut blocks = Vec::new();
    let mut events = Vec::new();
    let items: Vec<&str> = (0..6u64)
        .map(|i| ITEMS[((offset + i) % 16) as usize])
        .collect();
    if items.contains(&"create:creative_motor") {
        events.push(act(1, [0, 8, 0], Action::GameMode { creative: true }));
    }
    for (i, face) in FACES.into_iter().enumerate() {
        let cell = [i as i32 * 4 - 10, 8, 0];
        blocks.push(Placed {
            pos: cell,
            state: target.into(),
            motor: None,
        });
        events.push(click(
            6 + 2 * i as u32,
            cell,
            items[i],
            face,
            look(offset + i as u64),
            sneaking,
        ));
    }
    Scenario {
        version: 3,
        seed,
        day_time: 0,
        ticks: 90,
        checkpoints: vec![5, 19],
        blocks,
        events,
    }
}

/// Place each block item on a stone pedestal, then wrench it from one face, from the
/// opposite face and from a face across them (`IWrenchable.getRotatedBlockState` and each
/// block's override), with a creative motor beside it in half the indices. The last cell is
/// picked up with a sneaking wrench.
pub fn face_wrench(seed: u64, index: u64) -> Scenario {
    let item = ITEMS[(index % 16) as usize];
    let driven = index / 16 % 2 == 1;
    let mut blocks = Vec::new();
    let mut events = Vec::new();
    if item == "create:creative_motor" {
        events.push(act(1, [0, 8, 0], Action::GameMode { creative: true }));
    }
    let mut checkpoints = vec![30];
    for (i, face) in FACES.into_iter().enumerate() {
        let pedestal = [i as i32 * 4 - 10, 7, 0];
        let placed = [pedestal[0], 8, 0];
        blocks.push(Placed {
            pos: pedestal,
            state: "minecraft:stone".into(),
            motor: None,
        });
        if driven {
            blocks.push(Placed {
                pos: [placed[0], 8, -1],
                state: "create:creative_motor[facing=south]".into(),
                motor: Some(32),
            });
        }
        events.push(click(
            4 + i as u32,
            pedestal,
            item,
            Face::Up,
            look(index / 32 + i as u64),
            false,
        ));
        let opposite = FACES[i ^ 1];
        let across = FACES[(i + 2) % 6];
        for (k, f) in [face, opposite, across].into_iter().enumerate() {
            let tick = 32 + 12 * k as u32 + 2 * i as u32;
            events.push(click(tick, placed, "create:wrench", f, (0., 0.), false));
            checkpoints.push(tick + 1);
        }
        if i == 5 {
            events.push(click(80, placed, "create:wrench", face, (0., 0.), true));
        }
    }
    checkpoints.sort_unstable();
    Scenario {
        version: 3,
        seed,
        day_time: 0,
        ticks: 110,
        checkpoints,
        blocks,
        events,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement_meets_every_item_face_and_target() {
        let mut seen = std::collections::HashSet::new();
        for index in 0..192 {
            let sc = face_placement(index, index);
            let target = sc.blocks[0].state.clone();
            for e in &sc.events {
                if let Some(Action::Use {
                    item,
                    face,
                    sneaking,
                    ..
                }) = &e.action
                {
                    seen.insert((target.clone(), item.clone(), format!("{face:?}"), *sneaking));
                }
            }
        }
        assert_eq!(seen.len(), 6 * 16 * 6 * 2);
    }

    #[test]
    fn states_are_in_the_catalogue() {
        for index in 0..192 {
            for sc in [face_placement(index, index), face_wrench(index, index)] {
                for b in &sc.blocks {
                    crate::catalogue::validate(&b.state).unwrap();
                }
            }
        }
    }
}
