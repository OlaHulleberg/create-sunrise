//! The game around the breakers' glue (glue/processing/saw.rs reads a block's hardness, its tags
//! and a leaf's `distance`): the vanilla blocks of `breaking.json` in the driver's world, and
//! the game's own block ticks among them, a leaf's distance and a falling block. Each rule is
//! as the Java server ran it (docs/processing-scope.md#block-breaking); the leaf ticks are held
//! to the timelines of `leaves.json`, which the Java harness's `LeafDistance` replays at every
//! server start.
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

/// A scheduled block tick: the block it is for, where, and the level tick it is due.
struct Tick {
    pos: Pos,
    block: String,
    at: u32,
}

/// The level's scheduled block ticks (`LevelTicks`), in the order they were scheduled. One
/// tick a block at a position.
#[derive(Default)]
struct Ticks(Vec<Tick>);

impl Ticks {
    fn pending(&self, p: Pos, block: &str) -> bool {
        self.0.iter().any(|t| t.pos == p && t.block == block)
    }

    fn schedule(&mut self, tick: Tick) {
        if !self.pending(tick.pos, &tick.block) {
            self.0.push(tick);
        }
    }

    /// The ticks due by `now`, all taken out before any of them runs.
    fn due(&mut self, now: u32) -> Vec<Tick> {
        let (due, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.0)
            .into_iter()
            .partition(|t| t.at <= now);
        self.0 = waiting;
        due
    }
}

#[derive(Default)]
pub(super) struct Vanilla {
    /// Each catalogue block's state by position; the world map keeps `OTHER` there.
    pub states: HashMap<Pos, String>,
    ticks: Ticks,
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
            self.schedule_tick(p, &parse_state(&full).0, 2);
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

    /// `scheduleTick`: one pending tick a block at a position. A request made during a level tick
    /// runs `delay` ticks later, one made between ticks `delay` − 1 ticks after the next.
    fn schedule_tick(&mut self, p: Pos, block: &str, delay: u32) {
        self.vanilla.ticks.schedule(Tick {
            pos: p,
            block: block.to_owned(),
            at: self.now + delay,
        });
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
            let block = parse_state(&state).0;
            if falls(&state) {
                self.schedule_tick(q, &block, 2);
            } else if leaf(&state) {
                self.schedule_tick(q, &block, 1);
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

    /// The level's scheduled block ticks, before its entities and block entities. The ticks due
    /// are all taken out before any runs, in the order they were scheduled, and each runs only
    /// while its block is still at its position. A leaf that changes in the pass schedules the
    /// next tick of each neighbour, the ones still waiting in the pass included.
    pub(super) fn vanilla_ticks(&mut self) {
        for tick in self.vanilla.ticks.due(self.now) {
            let state = self.state_at(tick.pos);
            if parse_state(&state).0 != tick.block {
                continue;
            }
            if leaf(&state) {
                self.leaf_tick(tick.pos, &state);
            } else if falls(&state) {
                self.fall_tick(tick.pos, &state);
            }
        }
    }

    /// `LeavesBlock.tick`: 1 beside a log (`minecraft:logs`), else the least of the leaves
    /// around it one further, else 7. A leaf gets its tick when a neighbour changes; it does
    /// not decay: the server runs without random ticks.
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

    /// The leaf at `p` as the Java harness reads it for `leaves.json`: its `distance` and
    /// whether one of its ticks is pending; dots while another block is there.
    #[cfg(test)]
    pub(crate) fn leaf_cell(&self, p: Pos) -> (char, char) {
        let state = self.state_at(p);
        if !leaf(&state) {
            return ('.', '.');
        }
        let distance = crate::breaking::property(&state, "distance")
            .and_then(|d| d.parse::<u32>().ok())
            .and_then(|d| char::from_digit(d, 10))
            .unwrap_or('.');
        let pending = self.vanilla.ticks.pending(p, &parse_state(&state).0);
        (distance, if pending { '*' } else { '.' })
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
