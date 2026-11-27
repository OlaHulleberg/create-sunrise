//! Deployers, mirroring glue/processing/deployer.rs and fist.rs: the stroke's cycle and timing,
//! the recipes over a depot or a belt (`BeltDeployerCallbacks`), the fake player's use of the
//! held stack on the block it reaches and its punch (`DeployerHandler`), the filter, the item
//! handler and the saved state (`glue/processing/{filtering,containers,persist}.rs`).
//!
//! The fake player's action is the game's own (`perform_action_scoped` with `UseOn` or
//! `BreakBlock`): what the driver knows of that for the deployers' contract is `fake_use_on` and
//! `fake_break`, the vanilla rules the game runs as `DeployerAudit` measured them. Entities are
//! not in the harness, so none is used or attacked.
use super::*;
use create_engine::deployer::{self, Deployer, Depot, Mode, State};
use create_engine::kinetics::Dir;
use create_engine::processing;

use crate::deployers;
use crate::deployers::{Breaking, Held, Observation};

/// glue `deployer::OVERFLOW_SLOTS`.
const OVERFLOW_SLOTS: usize = 11;

fn step(p: Pos, d: Dir, n: i32) -> Pos {
    let o = d.offset();
    [p[0] + o[0] * n, p[1] + o[1] * n, p[2] + o[2] * n]
}

fn centre(p: Pos) -> [f64; 3] {
    p.map(|v| f64::from(v) + 0.5)
}

/// The state's block, without its properties.
fn block_of(state: &str) -> String {
    crate::scenario::parse_state(state).0
}

fn with_props(block: &str, props: Vec<(String, String)>) -> String {
    crate::scenario::format_state(block, &props)
}

/// `DeployerBlockEntity.activate`'s fake player looks the way its deployer faces.
fn fake_look(facing: Dir) -> (f32, f32) {
    match facing {
        Dir::Down => (0., 90.),
        Dir::Up => (0., -90.),
        Dir::North => (180., 0.),
        Dir::South => (0., 0.),
        Dir::West => (90., 0.),
        Dir::East => (-90., 0.),
    }
}

/// `deployer::along`: how far along its facing a click at `point` is from the deployer's back
/// face.
fn along(pos: Pos, facing: Dir, point: [f64; 3]) -> f64 {
    let o = facing.offset();
    (0..3)
        .map(|i| (point[i] - (f64::from(pos[i]) + 0.5 - 0.5 * f64::from(o[i]))) * f64::from(o[i]))
        .sum()
}

fn stack_of(h: &Held) -> crate::transport::Stack {
    crate::transport::Stack {
        item: h.item.clone(),
        count: h.count,
    }
}

impl Run {
    fn deployer(&self, p: Pos) -> Option<Deployer> {
        self.w.dims[0].deployers.get(&p).cloned()
    }

    fn deployer_facing(&self, p: Pos) -> Option<Dir> {
        self.w
            .table
            .get(self.at(p))
            .filter(|(b, _)| *b == create_engine::Block::Deployer)
            .and_then(|(_, i)| i.facing)
    }

    fn deployer_along_first(&self, p: Pos) -> bool {
        self.w
            .table
            .get(self.at(p))
            .is_some_and(|(_, i)| i.along_first)
    }

    /// The held stack with its wear (`held_stack`).
    fn held_of(&self, d: &Deployer) -> Option<Held> {
        d.held.clone().map(|(item, count)| Held {
            item,
            count,
            damage: d.held_damage,
        })
    }

    /// `set_held_stack`: the deployer's hand takes `held` (None empties it).
    fn set_hand(d: &mut Deployer, held: Option<Held>) {
        d.held = held.as_ref().map(|h| (h.item.clone(), h.count));
        d.held_damage = held.as_ref().map_or(0, |h| h.damage);
        if d.held.is_none() {
            d.held_damage = 0;
        }
    }

    fn has_overflow(&self, p: Pos) -> bool {
        self.deployer_overflow
            .get(&p)
            .is_some_and(|s| s.iter().any(Option::is_some))
    }

    /// `keep_overflow`: onto stacks of their kind first, then the empty slots; what finds no
    /// slot drops at the deployer.
    fn keep_overflow(&mut self, p: Pos, stacks: Vec<Held>) {
        if stacks.is_empty() {
            return;
        }
        let slots = self
            .deployer_overflow
            .entry(p)
            .or_insert([const { None }; OVERFLOW_SLOTS]);
        let before = slots.clone();
        let mut drops = Vec::new();
        for s in stacks {
            let mut left = Some(s);
            for slot in slots.iter_mut() {
                let Some(l) = left.take() else { break };
                match slot {
                    None => *slot = Some(l),
                    Some(have) if have.item == l.item => {
                        let max = deployers::limit(&have.item).unwrap_or(64);
                        let n = l.count.min(max.saturating_sub(have.count));
                        have.count += n;
                        left = (l.count > n).then(|| Held {
                            count: l.count - n,
                            ..l
                        });
                    }
                    Some(_) => left = Some(l),
                }
            }
            drops.extend(left);
        }
        if *slots != before {
            self.mark_column(p);
        }
        for l in drops {
            self.spawn_item(centre(p), [0.0; 3], &l.item, l.count);
        }
    }

    /// The deployer two above `depot`, facing down, and its speed (`deployer_above`).
    pub(super) fn deployer_above(&self, depot: Pos) -> Option<(Deployer, f32)> {
        let above = [depot[0], depot[1] + 2, depot[2]];
        if self.deployer_facing(above) != Some(Dir::Down) {
            return None;
        }
        Some((self.deployer(above)?, self.w.speed(0, above).unwrap_or(0.)))
    }

    /// `recipe_for`: sand paper polishes first (`SANDPAPER_POLISHING`), else the first of
    /// `create:deploying`, then the automatable `create:item_application`, whose item is `item`
    /// and whose tool `tool`.
    fn deployer_recipe(&self, item: &str, tool: &str) -> Option<deployer::Recipe> {
        let c = deployers::contract();
        if matches!(tool, "create:sand_paper" | "create:red_sand_paper")
            && let Some(p) = c
                .polishing()
                .iter()
                .find(|r| r.target.iter().any(|t| t == item))
        {
            let mut r = deployers::engine_recipe(p);
            r.tool = vec![tool.to_owned()];
            return Some(r);
        }
        c.deploying()
            .find(|r| r.target.iter().any(|t| t == item) && r.tool.iter().any(|t| t == tool))
            .map(deployers::engine_recipe)
    }

    /// `rolled`: `RecipeApplier.applyRecipeOn` on one of the stack, at most four results.
    fn rolled(&self, at: Pos, r: &deployer::Recipe) -> Vec<(String, u8)> {
        let seed = u64::from(self.now) ^ (at[0] as u64).wrapping_mul(0x9E37_79B9);
        let recipe = processing::Recipe {
            id: r.id.clone(),
            ingredients: Vec::new(),
            results: r.results.clone(),
            time: 0,
        };
        processing::roll(&recipe, &mut processing::Rng::new(seed))
            .into_iter()
            .take(4)
            .collect()
    }

    /// `start_over`: the stroke starting toward a depot or belt: the ray from its face to the
    /// top of the block two ahead (`start`).
    fn start_over(&mut self, at: Pos, d: &mut Deployer) {
        let top = deployers::contract()
            .block(&self.name(self.at(at)))
            .map_or(1.0, |b| b.top);
        d.start((1.0 - top) as f32);
        let above = [at[0], at[1] + 2, at[2]];
        let d2 = &mut self.w.dims[0];
        d2.deployers.insert(above, d.clone());
        d2.deployer_changes.push(above);
    }

    /// `press::results_on_depot`: one of the depot's held stack becomes `results`: a single
    /// item is replaced by them (the first held, the rest in the outputs), otherwise the rest
    /// stays held and the results go to the outputs. What stays held is centred; what finds no
    /// room drops above the depot. The held stack's motion after.
    fn results_on_depot(
        &mut self,
        depot: Pos,
        count: u8,
        held: create_engine::depot::Held,
        results: Vec<(String, u8)>,
    ) -> create_engine::depot::Held {
        use create_engine::depot::Held as Motion;
        let single = count == 1;
        let empty = results.is_empty();
        let mut next = held.centred();
        next.fan.clear();
        let mut rest = results.into_iter();
        let first: Option<crate::transport::Stack> = if single {
            rest.next().map(|(item, count)| {
                next = Motion {
                    locked: true,
                    ..held
                }
                .centred();
                crate::transport::Stack { item, count }
            })
        } else {
            self.depot_stacks
                .get(&depot)
                .map(|s| crate::transport::Stack {
                    count: s.count - 1,
                    ..s.clone()
                })
        };
        let mut dropped = Vec::new();
        if !(single && empty) {
            let outputs = self.depot_outputs.entry(depot).or_default();
            for (i, n) in rest {
                let left = outputs.insert_stacked(&crate::transport::Stack {
                    item: i.clone(),
                    count: n,
                });
                if left > 0 {
                    dropped.push((i, left));
                }
            }
            if outputs.is_empty() {
                self.depot_outputs.remove(&depot);
            }
        }
        match &first {
            Some(s) => {
                self.depot_stacks.insert(depot, s.clone());
            }
            None => {
                self.depot_stacks.remove(&depot);
            }
        }
        for (i, n) in dropped {
            self.spawn_item(
                [
                    f64::from(depot[0]) + 0.5,
                    f64::from(depot[1] + 1),
                    f64::from(depot[2]) + 0.5,
                ],
                [0.0; 3],
                &i,
                n,
            );
        }
        if empty && single {
            self.w.depot_put(0, depot, None);
        }
        self.w.depot_changed(0, depot);
        next
    }

    /// `deployer_used` after applying `recipe` (`BeltDeployerCallbacks.activate`): its tool
    /// wears, other items are used up (`Deployer::use_for`); a crafting remainder that finds no
    /// room in the hand goes into its overflow.
    fn deployer_used(&mut self, pos: Pos, d: &mut Deployer, recipe: &deployer::Recipe) {
        let held = self.held_of(d);
        let max_damage = held
            .as_ref()
            .and_then(|h| deployers::item(&h.item))
            .map_or(0, |i| i.max_damage);
        let remainder = held
            .as_ref()
            .and_then(|h| deployers::item(&h.item).and_then(|i| i.remainder.as_deref()));
        if let Some(rest) = d.use_for(recipe, max_damage, remainder) {
            self.keep_overflow(pos, vec![Held::new(&rest, 1)]);
        }
    }

    /// `BeltDeployerCallbacks` for the depot at `depot` at its turn (`deployer_on_depot`).
    pub(super) fn deployer_on_depot(&mut self, depot: Pos) {
        let Some((mut d, rpm)) = self.deployer_above(depot) else {
            return;
        };
        let above = [depot[0], depot[1] + 2, depot[2]];
        let Some(mut held) = self.w.dims[0].depots.get(&depot).copied().flatten() else {
            return;
        };
        if held.sliding() && held.position < 0.5 - 1.0 / 16.0 {
            return;
        }
        let Some(stack) = self.depot_stacks.get(&depot).cloned() else {
            return;
        };
        let tool = d.held.as_ref().map(|h| h.0.clone()).unwrap_or_default();
        let recipe = self.deployer_recipe(&stack.item, &tool);
        match deployer::on_depot(&d, rpm, held.locked, recipe.is_some()) {
            Depot::Pass => held.locked = false,
            Depot::Hold => held.locked = true,
            Depot::Start => {
                self.start_over(depot, &mut d);
                held.locked = true;
            }
            Depot::Apply => {
                let Some(r) = recipe else {
                    return;
                };
                let results = self.rolled(depot, &r);
                held = self.results_on_depot(depot, stack.count, held, results);
                held.locked = true;
                self.deployer_used(above, &mut d, &r);
                self.w.set_deployer(0, above, d);
            }
        }
        if let Some(h) = self.w.dims[0].depots.get_mut(&depot) {
            *h = h.map(|_| held);
        }
    }

    /// `BeltDeployerCallbacks` for a stack on the belt at `at` under the deployer `d` facing
    /// down at `rpm` (`deployer_on_belt`): the deployer starts its stroke on a stack its held
    /// item applies to, and back at the top of it turns one of the stack into the results.
    pub(super) fn deployer_on_belt(
        &mut self,
        at: Pos,
        item: &mut create_engine::belt::Item,
        locked: bool,
        mut d: Deployer,
        rpm: f32,
    ) -> create_engine::belt::Process {
        use create_engine::belt::Process;
        let above = [at[0], at[1] + 2, at[2]];
        let tool = d.held.as_ref().map(|h| h.0.clone()).unwrap_or_default();
        let recipe = self.deployer_recipe(&item.item, &tool);
        match deployer::on_depot(&d, rpm, locked, recipe.is_some()) {
            Depot::Pass => Process::Pass,
            Depot::Hold => Process::Hold,
            Depot::Start => {
                self.start_over(at, &mut d);
                Process::Hold
            }
            Depot::Apply => {
                let Some(r) = recipe else {
                    return Process::Pass;
                };
                let outputs: Vec<create_engine::belt::Item> = self
                    .rolled(at, &r)
                    .into_iter()
                    .map(|(name, n)| {
                        let mut out = item.processing_copy();
                        out.item = name;
                        out.count = n;
                        out.progress = None;
                        out
                    })
                    .collect();
                self.deployer_used(above, &mut d, &r);
                self.w.set_deployer(0, above, d);
                Process::Convert {
                    held: Some(item.left_of_one()),
                    outputs,
                }
            }
        }
    }

    /// `redstone_updates`: deployers whose neighbours changed are locked while powered.
    pub(super) fn deployer_redstone_updates(&mut self) {
        let power = std::mem::take(&mut self.w.dims[0].deployer_power_checks);
        for pos in power {
            if self.deployer_facing(pos).is_none() {
                continue;
            }
            let powered = self.signal(pos) > 0;
            let mut changed = false;
            if let Some(d) = self.w.dims[0].deployers.get_mut(&pos)
                && d.locked != powered
            {
                d.locked = powered;
                changed = true;
            }
            if changed {
                self.w.mark_unsaved(0, pos);
            }
        }
    }

    /// `deployer_turn`: the kinetic base tick, then [`Deployer::turn`].
    pub(super) fn deployer_turn(&mut self, pos: Pos) {
        self.w.kinetic_base_ticks_through(0, pos);
        let Some(facing) = self.deployer_facing(pos) else {
            return;
        };
        let Some(mut d) = self.deployer(pos) else {
            return;
        };
        let rpm = self.w.speed(0, pos).unwrap_or(0.);
        let changed = d.turn(
            rpm,
            &mut Turn {
                run: self,
                pos,
                facing,
            },
        );
        let dd = &mut self.w.dims[0];
        if !dd.deployers.contains_key(&pos) {
            return;
        }
        dd.deployers.insert(pos, d);
        if changed {
            dd.deployer_changes.push(pos);
            self.w.mark_unsaved(0, pos);
        }
    }

    /// Where the block a deployer holds would go (`deployer_place_target`), as a position;
    /// None when it holds no block, or nowhere fits.
    fn deployer_place_target(&self, pos: Pos, facing: Dir, d: &Deployer) -> Option<Pos> {
        let held = d.held.clone()?;
        let placed = deployers::item_block(&held.0)?;
        let reached = step(pos, facing, 2);
        let before = step(pos, facing, 1);
        let is_held = self.block_name(reached).as_deref() == Some(block_of(placed).as_str());
        deployer::placement(self.replaceable(reached), self.replaceable(before), is_held)
            .map(|at_reached| if at_reached { reached } else { before })
    }

    /// `shouldActivate` for a held item that is not a block: always, except a bucket of a fluid
    /// whose source is already in the block it reaches.
    fn deployer_uses(&self, pos: Pos, facing: Dir, d: &Deployer) -> bool {
        let Some((item, _)) = d.held.clone() else {
            return true;
        };
        if deployers::item_block(&item).is_some() {
            return false;
        }
        let reached = step(pos, facing, 2);
        let fluid = match item.as_str() {
            "minecraft:water_bucket" => "minecraft:water",
            "minecraft:lava_bucket" => "minecraft:lava",
            _ => return true,
        };
        self.block_name(reached).as_deref() != Some(fluid)
            || self.property(reached, "level").is_some_and(|l| l != "0")
    }

    /// The block at `p` by name, None for air.
    fn block_name(&self, p: Pos) -> Option<String> {
        (self.at(p) != AIR).then(|| block_of(&self.name(self.at(p))))
    }

    /// A property of the state at `p`.
    fn property(&self, p: Pos, key: &str) -> Option<String> {
        crate::scenario::parse_state(&self.name(self.at(p)))
            .1
            .into_iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    /// `DirectBeltInputBehaviour` at `p` (`takes_from_side`): a depot, an ejector or a belt
    /// segment.
    fn takes_from_side(&self, p: Pos) -> bool {
        let d = &self.w.dims[0];
        d.depots.contains_key(&p) || d.belt_of.contains_key(&p)
    }

    /// `DeployerBlockEntity.activate`: nothing over a depot or belt (their callbacks do the
    /// work); punching, `punch`; else the fake player clicks the block it reaches with what it
    /// holds (`activateInner` in `Mode.USE`), and the hand keeps what is left of the stack.
    fn deployer_activate(&mut self, pos: Pos, facing: Dir, d: &mut Deployer) {
        let reached = deployer::clicked(pos, facing);
        if facing == Dir::Down && self.takes_from_side(reached) {
            return;
        }
        if d.mode == Mode::Punch {
            self.punch(pos, facing, d);
        } else {
            self.fake_use_on(facing, reached, d);
        }
    }

    /// `activateInner` in `Mode.USE` on the block `reached` with the held stack: a manual item
    /// application turns it (`ManualApplicationRecipe`), a block item places its block there
    /// when it can be replaced (a fake player never places against a block: `BlockItemMixin`),
    /// an axe, a shovel or a hoe turns it, the wrench's use is the game's own; anything else
    /// is inert (`item.use` on air does nothing for the contract's items).
    fn fake_use_on(&mut self, facing: Dir, reached: Pos, d: &mut Deployer) {
        let Some(held) = self.held_of(d) else {
            return;
        };
        let state = self.name(self.at(reached));
        let block = block_of(&state);
        let contract = deployers::contract();
        if let Some(recipe) = contract
            .application()
            .iter()
            .find(|r| r.target.contains(&block) && r.tool.contains(&held.item))
        {
            self.apply_in_world(reached, &state, recipe, d);
            return;
        }
        if let Some(placed) = deployers::item_block(&held.item) {
            // `BlockItem.useOn` behind `BlockItemMixin`.
            if !self.replaceable(reached) {
                return;
            }
            let next = deployers::placed_state(placed, facing.opposite());
            if next.starts_with("create:") {
                let (block, _) = crate::scenario::parse_state(&next);
                let (yaw, pitch) = fake_look(facing);
                self.place_player(reached, &block, yaw, pitch, facing.opposite(), false);
            } else {
                self.set(reached, self.id(&next));
            }
            if self.replaceable(reached) {
                return;
            }
            let mut left = held;
            left.count -= 1;
            Self::set_hand(d, (left.count > 0).then_some(left));
            return;
        }
        if held.item == "create:wrench" {
            let (yaw, pitch) = fake_look(facing);
            let o = facing.offset();
            let hit = std::array::from_fn(|i| 0.5 - f64::from(o[i]) * 0.5);
            self.use_item(
                reached,
                "create:wrench",
                facing.opposite(),
                hit,
                yaw,
                pitch,
                false,
            );
            return;
        }
        // `Item.useOn` of an axe, a shovel or a hoe turns the block it is used on, keeping the
        // properties both blocks have (a log's axis); a shovel's passes on a block's bottom
        // face.
        if let Some(to) = deployers::item(&held.item).and_then(|i| i.turns.get(&block))
            && (facing != Dir::Up || deployers::item(&held.item).is_none_or(|i| i.turns_from_below))
            && let Some(next) = deployers::turned(&state, to)
        {
            self.set(reached, self.id(&next));
            d.wear(1, deployers::item(&held.item).map_or(0, |i| i.max_damage));
        }
    }

    /// `application::apply_in_world`: the block goes without its drops, the recipe's first
    /// result takes its place with the old block's properties, the rest pop out, and the held
    /// stack is used up unless the recipe keeps it.
    fn apply_in_world(
        &mut self,
        reached: Pos,
        state: &str,
        recipe: &deployers::Recipe,
        d: &mut Deployer,
    ) {
        let first = recipe.results.first();
        let to = first
            .and_then(|o| deployers::item_block(&o.item))
            .map(block_of);
        self.set(reached, AIR);
        if let Some(to) = &to {
            let declared = crate::defaults::declared(to);
            let mut next: Vec<(String, String)> = crate::scenario::parse_state(state)
                .1
                .into_iter()
                .filter(|(k, _)| declared.iter().any(|(dk, _)| dk == k))
                .collect();
            for (k, v) in declared {
                if !next.iter().any(|(nk, _)| *nk == k) {
                    next.push((k, v));
                }
            }
            let next = with_props(to, next);
            self.set(reached, self.id(&next));
        }
        for o in recipe.results.iter().skip(usize::from(to.is_some())) {
            self.spawn_item(centre(reached), [0.0; 3], &o.item, o.count);
        }
        if recipe.keep_held {
            return;
        }
        let Some(mut held) = self.held_of(d) else {
            return;
        };
        let item = deployers::item(&held.item);
        if item.is_some_and(|i| i.max_damage > 0) {
            d.wear(1, item.map_or(0, |i| i.max_damage));
            return;
        }
        held.count -= 1;
        let remainder = item.and_then(|i| i.remainder.clone());
        if held.count == 0 {
            Self::set_hand(d, remainder.map(|r| Held::new(&r, 1)));
        } else {
            Self::set_hand(d, Some(held));
        }
    }

    /// `fist::start` for the punching deployer `d` at `pos` at rest: true when it and the one
    /// facing it set off toward each other.
    fn fist_start(&mut self, pos: Pos, facing: Dir, d: &mut Deployer) -> bool {
        let Some((other, i)) = self.fist_partner(pos, facing) else {
            return false;
        };
        let faces_back = self.deployer_facing(other) == Some(facing.opposite());
        let punching = self.deployer(other).is_some_and(|d| d.mode == Mode::Punch);
        if !faces_back || !punching || self.w.speed(0, other).unwrap_or(0.) == 0. {
            return false;
        }
        let reach = (i - 2) as f32 * 0.5;
        fist_bump(d, reach);
        let dd = &mut self.w.dims[0];
        if let Some(dp) = dd.deployers.get_mut(&other) {
            fist_bump(dp, reach);
            dd.deployer_changes.push(other);
        }
        true
    }

    /// `fist::partner`: the first deployer two to four blocks ahead, and how far.
    fn fist_partner(&self, pos: Pos, facing: Dir) -> Option<(Pos, i32)> {
        (2..5).find_map(|i| {
            let p = step(pos, facing, i);
            self.w.dims[0].deployers.contains_key(&p).then_some((p, i))
        })
    }

    /// `fist::trigger` for the deployer `d` at `pos` at the end of its stroke: its partner
    /// there too is drawn back.
    fn fist_trigger(&mut self, pos: Pos, facing: Dir, d: &mut Deployer) {
        let Some((other, _)) = self.fist_partner(pos, facing) else {
            return;
        };
        if !self
            .deployer(other)
            .is_some_and(|o| o.fist_bump && o.state == State::Expanding && o.timer <= 0)
        {
            return;
        }
        d.fist_bump = false;
        let dd = &mut self.w.dims[0];
        if let Some(dp) = dd.deployers.get_mut(&other) {
            dp.fist_bump = false;
            dp.state = State::Retracting;
            dp.timer = 1000;
            dd.deployer_changes.push(other);
        }
    }

    /// `activateInner` in `Mode.PUNCH`: the block the deployer reaches breaks in strokes
    /// (`Deployer::punch`), a stroke's progress by `getDestroyProgress`; with an empty hand no
    /// progress is kept. A broken block is broken by the fake player with the held stack.
    fn punch(&mut self, pos: Pos, facing: Dir, d: &mut Deployer) {
        let at = deployer::clicked(pos, facing);
        // `getShape(..).isEmpty()`: nothing to punch, and the breaking is forgotten.
        if self.at(at) == AIR {
            d.breaking = None;
            return;
        }
        let Some(held) = d.held.clone() else {
            return;
        };
        let state = self.name(self.at(at));
        let Some(dig) = deployers::contract().dig(&held.0, &state) else {
            self.notes.push(format!(
                "adapter gap: a punch with {} on {state} at {at:?}",
                held.0
            ));
            return;
        };
        let hardness = deployers::contract()
            .block(&state)
            .and_then(|b| b.hardness)
            .unwrap_or(0.);
        let per_hit = dig.progress(hardness);
        if d.punch(at, per_hit) == deployer::Punch::Broken {
            self.fake_break(at, &held.0, d);
        }
    }

    /// `PlayerAction::BreakBlock` (`ServerPlayerGameMode.destroyBlock`) for the fake player
    /// holding `held`: the block goes, its drops lie where it stood when its stack harvests
    /// it, and a tool wears.
    fn fake_break(&mut self, at: Pos, held: &str, d: &mut Deployer) {
        let state = self.name(self.at(at));
        let dig = deployers::contract()
            .dig(held, &state)
            .expect("the punch's dig facts");
        self.set(at, AIR);
        if dig.harvest {
            for s in &dig.drops {
                self.pop_resource(at, &s.item, s.count);
            }
        }
        let max = deployers::item(held).map_or(0, |i| i.max_damage);
        if max > 0 {
            d.wear(1, max);
        }
    }

    /// The deployer's item handler as the port's game offers it (`containers`: eleven overflow
    /// slots, taken from on every face and nothing put in; the held stack is the engine's, not
    /// a slot).
    pub(super) fn deployer_capability(
        &mut self,
        p: Pos,
        slot: u8,
        extract: Option<u8>,
        simulate: bool,
        outcome: &mut crate::transport::Outcome,
    ) {
        outcome.handler = Some(true);
        let Some(n) = extract else {
            return;
        };
        let taken = self
            .deployer_overflow
            .get_mut(&p)
            .and_then(|slots| slots.get_mut(usize::from(slot)))
            .and_then(|s| {
                let have = s.as_mut()?;
                let k = n.min(have.count);
                let t = crate::transport::Stack {
                    item: have.item.clone(),
                    count: k,
                };
                if k == 0 {
                    return None;
                }
                if !simulate {
                    have.count -= k;
                    if have.count == 0 {
                        *s = None;
                    }
                }
                Some(t)
            });
        if taken.is_some() && !simulate {
            self.mark_column(p);
        }
        outcome.taken.extend(taken);
    }

    /// `DeployerObservations` for the deployer at `p`.
    pub(super) fn deployer_observation(&self, p: Pos) -> Option<Observation> {
        if self.w.block_at(0, p) != Some(create_engine::Block::Deployer) {
            return None;
        }
        let d = self.deployer(p).unwrap_or_default();
        Some(Observation {
            kind: "deployer".into(),
            mode: if d.mode == Mode::Punch {
                "punch"
            } else {
                "use"
            }
            .into(),
            state: match d.state {
                State::Waiting => "waiting",
                State::Expanding => "expanding",
                State::Retracting => "retracting",
            }
            .into(),
            timer: d.timer,
            reach: d.reach,
            held: self.held_of(&d),
            inventory: Vec::new(),
            overflow: self
                .deployer_overflow
                .get(&p)
                .map(|s| s.iter().flatten().cloned().collect())
                .unwrap_or_default(),
            locked: d.locked,
            owner: None,
            filter: self.funnel_filters.get(&p).cloned(),
            fist_bump: d.fist_bump,
            breaking: d.breaking.map(|(pos, progress)| Breaking { pos, progress }),
        })
    }

    /// `right_click_block` on the deployer at `p` (`use_block`): a filter slot takes the click
    /// (`filtering::click`), the wrench on its front half changes its mode (`onWrenched`), a
    /// front-half click swaps the player's hand with the deployer's (`useItemOn`). True when
    /// the click was the deployer's.
    pub(super) fn deployer_use(
        &mut self,
        p: Pos,
        item: &str,
        face: Dir,
        hit: [f64; 3],
        sneaking: bool,
    ) -> bool {
        let Some(facing) = self.deployer_facing(p) else {
            return false;
        };
        // The hit is relative to the block; `along` wants its point in the world.
        let point: [f64; 3] = std::array::from_fn(|i| hit[i] + f64::from(p[i]));
        if !sneaking
            && let Some(slot) = deployers::filter_slot(facing, self.deployer_along_first(p), face)
            && create_engine::board::hits_box(slot, hit)
        {
            if matches!(item, "create:wrench" | "create:mechanical_arm") {
                return true;
            }
            if item == "minecraft:air" {
                self.funnel_filters.remove(&p);
            } else {
                self.funnel_filters.insert(p, item.to_owned());
            }
            self.w.mark_unsaved(0, p);
            return true;
        }
        let front = along(p, facing, point).abs() > 0.75;
        if item == "create:wrench" {
            if !sneaking
                && front
                && let Some(mut d) = self.deployer(p)
            {
                d.mode = d.mode.toggled();
                self.w.set_deployer(0, p, d);
                return true;
            }
            return false;
        }
        if !front {
            return false;
        }
        let hand = (item != "minecraft:air").then(|| Held::new(item, item_count(item)));
        let Some(mut d) = self.deployer(p) else {
            return true;
        };
        if d.held.is_none() && hand.is_none() {
            return true;
        }
        Self::set_hand(&mut d, hand);
        self.w.set_deployer(0, p, d);
        true
    }

    /// A deployer gone (`DeployerBlockEntity.discardPlayer`): its overflow, the removed block
    /// entity's slots, and its held stack drop where the deployer stood (`drop_slot`).
    pub(super) fn deployer_removed(&mut self, p: Pos) {
        if let Some(slots) = self.deployer_overflow.remove(&p) {
            for s in slots.into_iter().flatten() {
                let stack = stack_of(&s);
                self.drop_item_stack(p.map(f64::from), &stack.item, stack.count);
            }
        }
        if let Some(dp) = self.w.dims[0].removed_deployers.remove(&p)
            && let Some((item, n)) = dp.held
        {
            self.drop_item_stack(p.map(f64::from), &item, n);
        }
        self.funnel_filters.remove(&p);
    }
}

/// `Actions.apply`: the stack a use hands the player: a bucket's one, anything else a full
/// stack of the item.
fn item_count(item: &str) -> u8 {
    if item.ends_with("_bucket") {
        1
    } else {
        deployers::limit(item).unwrap_or(64)
    }
}

fn fist_bump(d: &mut Deployer, reach: f32) {
    d.fist_bump = true;
    d.reach = reach;
    d.timer = 1000;
    d.state = State::Expanding;
}

/// The glue's `Turn`: the world around the deployer at `pos` for [`Deployer::turn`].
struct Turn<'a> {
    run: &'a mut Run,
    pos: Pos,
    facing: Dir,
}

impl deployer::World for Turn<'_> {
    fn lost_block(&mut self, at: Pos) -> bool {
        self.run.at(at) == AIR
    }

    fn overflowing(&mut self, d: &mut Deployer) -> bool {
        let (run, pos) = (&mut *self.run, self.pos);
        if run.has_overflow(pos) {
            return true;
        }
        // A held stack its filter doesn't let through leaves its hand into the overflow.
        let refused = run
            .funnel_filters
            .get(&pos)
            .is_some_and(|f| d.held.as_ref().is_some_and(|held| held.0 != *f));
        if !refused {
            return false;
        }
        let stack = run.held_of(d);
        Run::set_hand(d, None);
        run.keep_overflow(pos, stack.into_iter().collect());
        true
    }

    fn should_activate(&mut self, d: &Deployer) -> bool {
        let (run, pos, facing) = (&*self.run, self.pos, self.facing);
        if facing == Dir::Down && run.takes_from_side(step(pos, facing, 2)) {
            return false;
        }
        run.deployer_place_target(pos, facing, d).is_some() || run.deployer_uses(pos, facing, d)
    }

    fn start_fist_bump(&mut self, d: &mut Deployer) -> bool {
        self.run.fist_start(self.pos, self.facing, d)
    }

    fn ray_hit(&mut self) -> f32 {
        if self.run.replaceable(step(self.pos, self.facing, 1)) {
            0.75
        } else {
            0.
        }
    }

    fn trigger_fist_bump(&mut self, d: &mut Deployer) {
        self.run.fist_trigger(self.pos, self.facing, d);
    }

    fn activate(&mut self, d: &mut Deployer) {
        self.run.deployer_activate(self.pos, self.facing, d);
    }
}
