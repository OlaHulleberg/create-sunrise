//! The game around the breakers' glue (glue/processing/saw.rs reads a block's hardness, its tags
//! and a leaf's `distance`): the vanilla blocks of `breaking.json` in the driver's world, and
//! the game's own block ticks among them, a leaf's distance and a falling block. Each rule is
//! as the Java server ran it (docs/processing-scope.md#block-breaking).
use super::*;
use crate::breaking::Loot;
use crate::transport::{Placed, Spreader};

/// A falling block entity: its block, and where it is (`FallingBlockEntity`).
struct Falling {
    column: [i32; 2],
    state: String,
    at: f64,
    speed: f64,
}

#[derive(Default)]
pub(super) struct Vanilla {
    /// Each catalogue block's state by position; the world map keeps `OTHER` there.
    pub states: HashMap<Pos, String>,
    /// The scheduled block ticks: position, the level tick they are due, and their order.
    ticks: Vec<(Pos, u32, u32)>,
    order: u32,
    falling: Vec<Falling>,
}

/// `Direction.values()`: the order the game tells neighbours in.
const ORDER: [Pos; 6] = [
    [0, -1, 0],
    [0, 1, 0],
    [0, 0, -1],
    [0, 0, 1],
    [-1, 0, 0],
    [1, 0, 0],
];

fn leaf(state: &str) -> bool {
    crate::breaking::block(&parse_state(state).0).is_some_and(|b| b.leaf)
}

fn falls(state: &str) -> bool {
    crate::breaking::block(&parse_state(state).0).is_some_and(|b| b.falls)
}

/// A huge mushroom block's face towards `ORDER`'s neighbour, each looking back at it.
const MUSHROOM_FACE: [&str; 6] = ["up", "down", "south", "north", "east", "west"];

fn mushroom(state: &str) -> bool {
    MUSHROOM_FACE
        .iter()
        .all(|face| crate::breaking::property(state, face).is_some())
}

impl Run {
    /// The block at `p` as `/setblock` writes it.
    pub(super) fn state_at(&self, p: Pos) -> String {
        self.vanilla
            .states
            .get(&p)
            .cloned()
            .unwrap_or_else(|| self.name(self.at(p)))
    }

    /// A block placed or replaced with `state`: a catalogue block keeps its state here, anything
    /// else goes through the world. Either way the game tells the neighbours (`set_block`, 3).
    pub(super) fn set_state(&mut self, p: Pos, state: &str) {
        let full = (self.id(state) == OTHER)
            .then(|| crate::breaking::full_state(state))
            .flatten();
        let Some(full) = full else {
            let id = self.id(state);
            self.set(p, id);
            return;
        };
        if self.vanilla.states.get(&p) == Some(&full) && self.at(p) == OTHER {
            return;
        }
        // A block of the engine's there goes by the world's own path first.
        if self.at(p) != OTHER {
            self.vanilla.states.remove(&p);
            self.commit(p, OTHER, false);
        } else {
            self.world.insert(p, OTHER);
        }
        self.vanilla.states.insert(p, full.clone());
        self.fluid_world_changed(p);
        // `onPlace` runs before the neighbours' `updateShape`.
        if falls(&full) {
            self.schedule_tick(p, 2);
        }
        // The game breaks a plant on a block it does not stand on at a later tick; not followed.
        if let Some(b) = crate::breaking::block(&parse_state(&full).0)
            && !b.stands_on.is_empty()
            && !b
                .stands_on
                .iter()
                .any(|g| *g == parse_state(&self.state_at([p[0], p[1] - 1, p[2]])).0)
        {
            self.notes.push(format!(
                "adapter gap: {full} at {p:?} does not stand on the block below"
            ));
        }
        self.neighbours_changed(p, OTHER);
    }

    /// `scheduleTick`: one pending tick a position. A request made during a level tick runs
    /// `delay` ticks later, one made between ticks `delay` − 1 ticks after the next.
    fn schedule_tick(&mut self, p: Pos, delay: u32) {
        if self.vanilla.ticks.iter().any(|(q, ..)| *q == p) {
            return;
        }
        self.vanilla.order += 1;
        self.vanilla
            .ticks
            .push((p, self.now + delay, self.vanilla.order));
    }

    /// `updateShape` of the six neighbours, in the game's order: a leaf schedules its distance,
    /// a falling block its fall, and a mushroom block's face turns to its skin towards a block
    /// of its own kind that comes (`HugeMushroomBlock.updateShape`); a block that changes
    /// tells its neighbours again.
    pub(super) fn tell_neighbours(&mut self, p: Pos) {
        for (d, face) in ORDER.iter().zip(MUSHROOM_FACE) {
            let q = add(p, *d);
            let Some(state) = self.vanilla.states.get(&q).cloned() else {
                continue;
            };
            if falls(&state) {
                self.schedule_tick(q, 2);
            } else if leaf(&state) {
                self.schedule_tick(q, 1);
            }
            // A face that turned to its skin stays so when the other block goes.
            if mushroom(&state)
                && crate::breaking::property(&state, face).is_some_and(|v| v == "true")
                && parse_state(&self.state_at(p)).0 == parse_state(&state).0
            {
                let next = crate::breaking::with(&state, face, "false");
                self.set_state(q, &next);
            }
        }
    }

    /// The level's scheduled block ticks, before its entities and block entities. Each is taken
    /// out before it runs: a tick that ran can be scheduled again within the pass, one still
    /// waiting cannot.
    pub(super) fn vanilla_ticks(&mut self) {
        let mut due: Vec<(Pos, u32)> = self
            .vanilla
            .ticks
            .iter()
            .filter(|(_, at, _)| *at <= self.now)
            .map(|(p, _, order)| (*p, *order))
            .collect();
        if due.is_empty() {
            return;
        }
        due.sort_by_key(|(_, order)| *order);
        for (p, _) in due {
            let index = self.vanilla.ticks.iter().position(|(q, ..)| *q == p);
            let Some(index) = index else {
                continue;
            };
            self.vanilla.ticks.swap_remove(index);
            let state = self.state_at(p);
            if leaf(&state) {
                self.leaf_tick(p, &state);
            } else if falls(&state) {
                self.fall_tick(p, &state);
            }
        }
    }

    /// `LeavesBlock.tick`: 1 beside a log (`minecraft:logs`), else the least of the leaves
    /// around it one further, else 7. The leaf does not decay: the server runs without random
    /// ticks.
    fn leaf_tick(&mut self, p: Pos, state: &str) {
        let mut best = 7;
        for d in ORDER {
            let other = self.state_at(add(p, d));
            if !crate::breaking::known(&other) {
                continue;
            }
            if crate::breaking::tagged(&other, "minecraft:logs") {
                best = 1;
                break;
            }
            if leaf(&other) {
                best = best.min(
                    crate::breaking::property(&other, "distance")
                        .and_then(|v| v.parse::<i32>().ok())
                        .unwrap_or(7)
                        + 1,
                );
            }
        }
        let text = best.to_string();
        if crate::breaking::property(state, "distance").as_deref() != Some(text.as_str()) {
            let next = crate::breaking::with(state, "distance", &text);
            self.set_state(p, &next);
        }
    }

    fn free(&self, p: Pos) -> bool {
        self.at(p) == AIR
    }

    /// `FallingBlock.tick`: with nothing below it, the block becomes a falling entity.
    fn fall_tick(&mut self, p: Pos, state: &str) {
        if !self.free([p[0], p[1] - 1, p[2]]) {
            return;
        }
        self.vanilla.falling.push(Falling {
            column: [p[0], p[2]],
            state: state.to_owned(),
            at: f64::from(p[1]),
            speed: 0.0,
        });
        self.set(p, AIR);
    }

    /// `FallingBlockEntity.tick` for each, in the entity pass: 0.04 faster a tick, dragged by
    /// 0.98. Resting on the block below the cell it is in, it becomes that cell's block.
    pub(super) fn vanilla_entities(&mut self) {
        let mut keep = Vec::new();
        let mut resting = Vec::new();
        for mut e in std::mem::take(&mut self.vanilla.falling) {
            let step = e.speed + 0.04;
            e.speed = step * 0.98;
            let cell = e.at.floor() as i32;
            let next = e.at - step;
            let below = [e.column[0], cell - 1, e.column[1]];
            if next < f64::from(cell) && !self.free(below) {
                resting.push(([e.column[0], cell, e.column[1]], e.state));
                continue;
            }
            e.at = next;
            keep.push(e);
        }
        self.vanilla.falling = keep;
        for (p, state) in resting {
            if self.free(p) {
                self.set_state(p, &state);
            } else {
                self.broken_fall(p, &state);
            }
        }
    }

    /// A second falling block comes to rest in the cell another one turned into its block this
    /// tick: it turns into its item at the cell's floor, rising (chance decides the item where
    /// it decides the block's loot, which the comparison leaves out).
    fn broken_fall(&mut self, p: Pos, state: &str) {
        let at = [
            f64::from(p[0]) + 0.5,
            f64::from(p[1]),
            f64::from(p[2]) + 0.5,
        ];
        if let Some(Loot::Fixed(stacks)) = crate::breaking::loot(state) {
            for (item, count) in stacks {
                self.join(
                    at,
                    [0.0; 3],
                    &item,
                    count,
                    Placed::Spread(Spreader::BrokenFall),
                );
            }
        }
    }
}
