//! Pumps driven by motor/cog inputs; sources filled through ordinary player bucket use.
use crate::{
    actions::{Action, Face},
    scenario::{Change, Placed, Scenario},
};
pub fn generate(seed: u64) -> Scenario {
    let rpm = match (seed / 32) % 4 {
        0 => 32,
        1 => -32,
        2 => 8,
        _ => 128,
    };
    let reaction = (seed / 1792) % 4;
    if reaction != 0 {
        return reactions(seed, reaction, rpm);
    }
    let lane = (seed / 128) % 14;
    let length = match lane {
        1 => 7,
        2 => 15,
        3 => 16 + i32::from(!(seed / 1152).is_multiple_of(2)),
        5 => 2,
        9 => 3,
        _ => 1,
    };
    let mut sc = Scenario {
        version: 3,
        seed,
        ticks: 260,
        day_time: 0,
        checkpoints: vec![],
        blocks: vec![],
        events: vec![],
    };
    sc.blocks.push(Placed {
        pos: [if lane == 13 { 0 } else { 1 }, 4, -1],
        state: "create:creative_motor[facing=east]".into(),
        motor: Some(rpm),
    });
    sc.blocks.push(Placed {
        pos: [2, 4, -1],
        state: "create:cogwheel[axis=x,waterlogged=false]".into(),
        motor: None,
    });
    if lane == 9 {
        sc.blocks.push(Placed {
            pos: [3, 4, -1],
            state: "create:creative_motor[facing=east]".into(),
            motor: Some(rpm),
        });
        sc.blocks.push(Placed {
            pos: [4, 4, -1],
            state: "create:cogwheel[axis=x,waterlogged=false]".into(),
            motor: None,
        });
    }
    if lane == 13 {
        sc.blocks.push(Placed {
            pos: [1, 4, -1],
            state: "create:clutch[axis=x,powered=true]".into(),
            motor: None,
        });
        sc.blocks.push(Placed {
            pos: [1, 5, -1],
            state: "minecraft:redstone_block".into(),
            motor: None,
        });
    }
    let target = 3 + length;
    let mut placements = vec![
        ([1, 3, 0], "create:fluid_pipe"),
        ([2, 3, 0], "create:mechanical_pump"),
    ];
    for x in 3..target {
        placements.push((
            [x, 3, 0],
            if lane == 9 && x == 4 {
                "create:mechanical_pump"
            } else {
                "create:fluid_pipe"
            },
        ));
    }
    if lane != 7 && lane != 10 {
        placements.push(([target, 3, 0], "create:fluid_tank"));
    }
    if lane == 4 {
        placements.push(([3, 3, 1], "create:fluid_tank"));
    }
    if lane == 5 {
        for x in 3..=5 {
            placements.push(([x, 3, -1], "create:fluid_pipe"));
        }
    }
    for x in 0..=target {
        sc.blocks.push(Placed {
            pos: [x, 3, 0],
            state: "minecraft:stone".into(),
            motor: None,
        });
    }
    for (p, _) in &placements {
        if p[2] != 0 {
            sc.blocks.push(Placed {
                pos: *p,
                state: "minecraft:stone".into(),
                motor: None,
            });
        }
    }
    if matches!(lane, 7 | 8 | 10 | 11) {
        for x in -1..=6 {
            for z in -3..=2 {
                sc.blocks.push(Placed {
                    pos: [x, 3, z],
                    state: "minecraft:stone".into(),
                    motor: None,
                });
                if x == -1 || x == 6 || z == -3 || z == 2 {
                    for y in [4, 5] {
                        sc.blocks.push(Placed {
                            pos: [x, y, z],
                            state: "minecraft:stone".into(),
                            motor: None,
                        });
                    }
                }
            }
        }
    }
    if lane == 10 {
        sc.blocks.push(Placed {
            pos: [target, 4, 0],
            state: "minecraft:stone_slab[type=bottom,waterlogged=false]".into(),
            motor: None,
        });
    }
    if lane != 8 {
        sc.blocks.push(Placed {
            pos: [0, 4, 0],
            state: if lane == 13 {
                "minecraft:beehive[facing=north,honey_level=5]"
            } else if lane == 11 {
                "minecraft:stone_slab[type=bottom,waterlogged=false]"
            } else {
                "minecraft:cauldron"
            }
            .into(),
            motor: None,
        });
    }
    let start = (sc.blocks.len() as u32 + 8).max(20);
    let fill = start + (placements.len() as u32 - 1) * 4 + 28;
    let use_at = |tick, pos, item: &str, face: Face| Change {
        tick,
        pos,
        state: None,
        motor: None,
        turn: None,
        action: Some(Action::Use {
            item: item.into(),
            face,
            hit: face.point(0.5, 0.5),
            yaw: 90.,
            pitch: 0.,
            sneaking: false,
        }),
    };
    for (i, (p, item)) in placements.into_iter().enumerate() {
        sc.events
            .push(use_at(start + i as u32 * 4, p, item, Face::Up));
    }
    if lane == 13 {
        sc.events.push(Change {
            tick: fill,
            pos: [1, 5, -1],
            state: None,
            motor: None,
            turn: None,
            action: Some(Action::Remove),
        });
    } else {
        sc.events.push(use_at(
            fill,
            if lane == 8 { [0, 3, 0] } else { [0, 4, 0] },
            "minecraft:water_bucket",
            Face::Up,
        ));
    }
    if lane == 6 {
        sc.events
            .push(use_at(fill + 70, [2, 4, 0], "create:wrench", Face::Up));
    }
    if lane == 8 || lane == 11 {
        sc.events.push(Change {
            tick: fill + 160,
            pos: [1, 4, -1],
            state: None,
            motor: None,
            turn: None,
            action: Some(Action::Remove),
        });
    }
    if lane == 12 {
        sc.events
            .push(use_at(fill + 40, [1, 4, 0], "create:wrench", Face::North));
        sc.events.push(use_at(
            fill + 48,
            [1, 4, 0],
            "create:copper_casing",
            Face::North,
        ));
        sc.events
            .push(use_at(fill + 56, [1, 4, 0], "create:wrench", Face::North));
    }
    sc.ticks = fill
        + (if lane == 8 || lane == 11 {
            240u32
        } else {
            220u32
        })
        .max((1000 / (rpm.unsigned_abs() / 2).max(1)) + 150);
    sc.checkpoints = (start..=fill + 180).collect();
    sc
}

fn placed(pos: [i32; 3], state: &str, motor: Option<i32>) -> Placed {
    Placed {
        pos,
        state: state.into(),
        motor,
    }
}

/// Pipe flows of two fluids meeting in a pipe (`FluidReactions.handlePipeFlowCollision`, variant 1:
/// water from the west and lava from the east into a junction over a tank) and a pipe pouring
/// into a source of another fluid (`handlePipeSpillCollision`: water into lava, 2; lava into
/// water, 3). Sources are full cauldrons; every fluid cell is walled in.
fn reactions(seed: u64, variant: u64, rpm: i32) -> Scenario {
    let mut sc = Scenario {
        version: 3,
        seed,
        ticks: 0,
        day_time: 0,
        checkpoints: vec![],
        blocks: vec![],
        events: vec![],
    };
    let end = if variant == 1 {
        8
    } else {
        3 + (seed / 7168 % 3) as i32
    };
    for x in 0..=end {
        sc.blocks.push(placed([x, 3, 0], "minecraft:stone", None));
    }
    sc.blocks.push(placed(
        [1, 4, -1],
        "create:creative_motor[facing=east]",
        Some(rpm),
    ));
    sc.blocks.push(placed(
        [2, 4, -1],
        "create:cogwheel[axis=x,waterlogged=false]",
        None,
    ));
    let (west, east) = match variant {
        1 => (
            "minecraft:water_cauldron[level=3]",
            "minecraft:lava_cauldron",
        ),
        2 => (
            "minecraft:water_cauldron[level=3]",
            "minecraft:lava[level=0]",
        ),
        _ => ("minecraft:lava_cauldron", "minecraft:water[level=0]"),
    };
    sc.blocks.push(placed([0, 4, 0], west, None));
    let mut placements = vec![
        ([1, 3, 0], "create:fluid_pipe", 90.),
        ([2, 3, 0], "create:mechanical_pump", 90.),
    ];
    if variant == 1 {
        sc.blocks.push(placed(
            [5, 4, -1],
            "create:creative_motor[facing=east]",
            Some(rpm),
        ));
        sc.blocks.push(placed(
            [6, 4, -1],
            "create:cogwheel[axis=x,waterlogged=false]",
            None,
        ));
        sc.blocks.push(placed([4, 3, 1], "minecraft:stone", None));
        sc.blocks.push(placed([8, 4, 0], east, None));
        placements.insert(0, ([4, 3, 1], "create:fluid_tank", 90.));
        placements.extend([
            ([3, 3, 0], "create:fluid_pipe", 90.),
            ([4, 3, 0], "create:fluid_pipe", 90.),
            ([5, 3, 0], "create:fluid_pipe", 270.),
            ([6, 3, 0], "create:mechanical_pump", 270.),
            ([7, 3, 0], "create:fluid_pipe", 270.),
        ]);
    } else {
        for p in [[end, 4, -1], [end, 4, 1], [end + 1, 4, 0], [end, 5, 0]] {
            sc.blocks.push(placed(p, "minecraft:stone", None));
        }
        for x in 3..end {
            placements.push(([x, 3, 0], "create:fluid_pipe", 90.));
        }
    }
    let start = sc.blocks.len() as u32 + 8;
    for (i, (p, item, yaw)) in placements.iter().enumerate() {
        sc.events.push(Change {
            tick: start + i as u32 * 4,
            pos: *p,
            state: None,
            motor: None,
            turn: None,
            action: Some(Action::Use {
                item: (*item).into(),
                face: Face::Up,
                hit: Face::Up.point(0.5, 0.5),
                yaw: *yaw,
                pitch: 0.,
                sneaking: false,
            }),
        });
    }
    let built = start + placements.len() as u32 * 4;
    if variant != 1 {
        // After the last pipe walls its cell in.
        sc.events.push(Change {
            tick: built,
            pos: [end, 4, 0],
            state: Some(east.into()),
            motor: None,
            turn: None,
            action: None,
        });
    }
    sc.ticks = built + 240;
    sc.checkpoints = (built..built + 200).step_by(2).collect();
    sc
}
