//! The block breakers, mirroring glue/processing/saw.rs (`tick_drills`, `fell`, `sawable`,
//! `cell`): `Tools::level_tick` runs them after logistics' walk and before the presses.
//! `BlockBreakingKineticBlockEntity`'s rules are the engine's (`drill`, `tree`); this reads the
//! world for them and writes what the glue asks of the host.
use super::*;
use crate::breaking::{Breaking, Loot};
use crate::transport::{Placed as Drop, Spreader};
use create_engine::kinetics::{Axis, Dir};
use create_engine::{Block, drill, tree};

/// The game rules a case sets: `doTileDrops` (`RULE_DOBLOCKDROPS`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Rules {
    pub do_tile_drops: bool,
}

impl Default for Rules {
    fn default() -> Self {
        Rules {
            do_tile_drops: true,
        }
    }
}

fn offset(d: Dir) -> Pos {
    d.offset()
}

impl Run {
    /// `saw::tick_drills`, over the turning breakers in the engine's order.
    pub(super) fn tick_drills(&mut self) {
        let drilling = self.w.dims[0].drilling.clone();
        for pos in drilling {
            let Some((block, info)) = self.w.table.get(self.at(pos)) else {
                continue;
            };
            let Some(facing) = info.facing else {
                continue;
            };
            let saw = block == Block::MechanicalSaw;
            if saw && facing.axis() == Axis::Y {
                continue;
            }
            let at = add(pos, offset(facing));
            let state = self.state_at(at);
            let fluid = crate::fluid_world::observe(&state).is_some_and(|o| o.amount > 0);
            let hardness = if self.at(at) == AIR
                || fluid
                || crate::breaking::tagged(&state, "create:non_breakable")
                || (saw && !Self::sawable(&state))
            {
                None
            } else {
                crate::breaking::hardness(&state).and_then(drill::breakable)
            };
            let speed = self.w.speed(0, pos).unwrap_or(0.0);
            let step = self.w.dims[0].drills.get_mut(&pos).map(|d| {
                if d.lazy.tick() {
                    d.lazy();
                }
                d.tick(speed, hardness)
            });
            let Some(step) = step else {
                continue;
            };
            match step {
                drill::Step::Wait => {}
                drill::Step::Progress(stage) => self.set_break_progress(pos, at, stage),
                drill::Step::Reset => self.set_break_progress(pos, at, -1),
                drill::Step::Broken => {
                    self.set_break_progress(pos, at, -1);
                    let broken = Self::cell(&state);
                    let centre = at.map(|v| f64::from(v) + 0.5);
                    for (item, count) in self.destroy_collect(at) {
                        self.breaker_drop(centre, &item, count, [0.0; 3]);
                    }
                    if saw {
                        self.fell(pos, at, broken);
                    }
                }
            }
        }
    }

    /// `lv.level.set_break_progress`: what the host shows of the breaker's cracks (-1: none).
    fn set_break_progress(&mut self, breaker: Pos, at: Pos, stage: i32) {
        if stage < 0 {
            self.cracks.remove(&breaker);
        } else {
            self.cracks.insert(breaker, (at, stage));
        }
    }

    /// `destroy_collect`: the host's `destroy_block(pos, true, ..)` gives the block's loot (none
    /// with `doTileDrops` off) and replaces the block.
    fn destroy_collect(&mut self, p: Pos) -> Vec<(String, u8)> {
        let state = self.state_at(p);
        if self.at(p) == AIR {
            return Vec::new();
        }
        let loot = if self.rules.do_tile_drops {
            crate::breaking::loot(&state)
        } else {
            Some(Loot::Fixed(Vec::new()))
        };
        if state.contains("waterlogged=true") {
            self.notes
                .push(format!("adapter gap: the fluid of {state} at {p:?}"));
        }
        self.set(p, AIR);
        match loot {
            None => {
                self.notes
                    .push(format!("adapter gap: loot of {state} at {p:?}"));
                Vec::new()
            }
            Some(Loot::Fixed(stacks)) => stacks,
            Some(Loot::Rolled(_)) => Vec::new(),
        }
    }

    /// `lv.spawn_stack` of a broken block's drops: at the block's middle, standing still.
    fn breaker_drop(&mut self, at: [f64; 3], item: &str, count: u8, motion: [f64; 3]) {
        let placed = if self.entity_pass_done {
            Drop::Spread(Spreader::Centre)
        } else {
            Drop::Flown
        };
        self.join(at, motion, item, count, placed);
    }

    /// `sawable` (`SawBlockEntity.isSawable`): logs, leaves, roots, bamboo, the solid crops and
    /// the chorus plant; not saplings.
    fn sawable(state: &str) -> bool {
        if crate::breaking::tagged(state, "minecraft:saplings") {
            return false;
        }
        let c = Self::cell(state);
        c.log
            || c.root
            || c.vertical
            || crate::breaking::tagged(state, "minecraft:leaves")
            || crate::breaking::tagged(state, "create:solid_crops")
            || parse_state(state).0 == "minecraft:chorus_plant"
    }

    /// `cell`: what `TreeCutter` reads of the block.
    fn cell(state: &str) -> tree::Cell {
        let (name, _) = parse_state(state);
        tree::Cell {
            log: crate::breaking::tagged(state, "minecraft:logs")
                || crate::breaking::tagged(state, "tconstruct:slimy_logs")
                || name == "minecraft:mushroom_stem",
            root: crate::breaking::tagged(state, "create:roots"),
            leaf: if name == "minecraft:scaffolding" {
                None
            } else {
                crate::breaking::property(state, "distance").and_then(|d| d.parse().ok())
            },
            vertical: crate::breaking::contract().vertical.contains(&name),
            chorus: matches!(
                name.as_str(),
                "minecraft:chorus_plant" | "minecraft:chorus_flower"
            ),
            attachment: crate::breaking::tagged(state, "create:tree_attachments"),
            non_decaying: match name.as_str() {
                "minecraft:red_mushroom_block" => 2,
                "minecraft:brown_mushroom_block"
                | "minecraft:weeping_vines"
                | "minecraft:weeping_vines_plant" => 3,
                _ if crate::breaking::tagged(state, "minecraft:wart_blocks") => 3,
                _ => -1,
            },
        }
    }

    /// `fell`: `TreeCutter.findTree(..).destroyBlocks(.., dropItemFromCutTree)` for the block
    /// the saw at `saw` cut at `at`: what stood on it falls, its drops flying on from each
    /// block's middle the way the saw faces.
    fn fell(&mut self, saw: Pos, at: Pos, broken: tree::Cell) {
        let (min_y, height) = (-64, 384);
        let top = min_y + height;
        let found = tree::find(at, broken, height, |p| {
            if p[1] < min_y || p[1] >= top {
                return tree::Cell {
                    non_decaying: -1,
                    ..tree::Cell::default()
                };
            }
            Self::cell(&self.state_at(p))
        });
        let push = [at[0] - saw[0], at[1] - saw[1], at[2] - saw[2]].map(f64::from);
        for (p, _) in found.blocks() {
            if self.at(p) == AIR {
                continue;
            }
            let distance = (0..3)
                .map(|i| f64::from(p[i] - at[i]).powi(2))
                .sum::<f64>()
                .sqrt();
            let centre = p.map(|v| f64::from(v) + 0.5);
            let v = push.map(|c| c * distance / 20.0);
            for (item, count) in self.destroy_collect(p) {
                self.spawn_item(centre, v, &item, count);
            }
        }
    }

    /// The breakers' `Progress` and `NextTick` of the block at `p`, and the cracks at it.
    pub(super) fn breaking_observation(&self, p: Pos) -> Option<Breaking> {
        matches!(
            self.w.block_at(0, p),
            Some(Block::MechanicalDrill | Block::MechanicalSaw)
        )
        .then(|| {
            self.w.dims[0]
                .drills
                .get(&p)
                .map(|d| Breaking {
                    progress: d.progress,
                    next_tick: d.ticks_until,
                })
                .unwrap_or(Breaking {
                    progress: 0,
                    next_tick: 0,
                })
        })
    }

    /// The crack stages shown at `p`.
    pub(super) fn cracks_at(&self, p: Pos) -> Option<Vec<i32>> {
        let mut stages: Vec<i32> = self
            .cracks
            .values()
            .filter(|(at, _)| *at == p)
            .map(|(_, stage)| *stage)
            .collect();
        if stages.is_empty() {
            return None;
        }
        stages.sort_unstable();
        Some(stages)
    }
}
