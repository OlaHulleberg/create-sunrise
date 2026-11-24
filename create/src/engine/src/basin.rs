//! Create's basin and the mechanical mixer over it (`BasinBlockEntity`, `BasinRecipe`,
//! `BasinOperatingBlockEntity`, `MechanicalMixerBlockEntity`, `HeatCondition`): items in the
//! basin's nine input slots, a recipe whose ingredients they hold (and whose heat the burner
//! below gives) is mixed in a cycle of the mixer's head, its results going to the nine outputs.

use crate::Block;
use crate::fan::Heat;
use crate::processing::{Output, Rng, Slot, insert_stacked};
use crate::tank::Fluid;
use crate::world::Dimension;
use std::collections::{HashMap, HashSet};

/// `BasinBlockEntity`'s fluid tanks: two inputs and two outputs of a bucket each.
pub const TANK: u32 = 1000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tanks {
    pub input: [Option<(Fluid, u32)>; 2],
    pub output: [Option<(Fluid, u32)>; 2],
}

/// Puts up to `amount` of `fluid` in one of `tanks` (one holding it, else an empty one): what
/// went in.
pub fn fill(tanks: &mut [Option<(Fluid, u32)>; 2], fluid: Fluid, amount: u32) -> u32 {
    let i = tanks
        .iter()
        .position(|t| t.is_some_and(|(f, _)| f == fluid))
        .or_else(|| tanks.iter().position(Option::is_none));
    let Some(i) = i else {
        return 0;
    };
    let have = tanks[i].map_or(0, |t| t.1);
    let took = amount.min(TANK - have);
    if took > 0 {
        tanks[i] = Some((fluid, have + took));
    }
    took
}

/// Takes up to `amount` from the first non-empty tank: the fluid and how much.
pub fn drain(tanks: &mut [Option<(Fluid, u32)>; 2], amount: u32) -> Option<(Fluid, u32)> {
    drain_matching(tanks, amount, None)
}

/// Fluid capability drain with an optional requested fluid (FlowSource's filter fallback).
pub fn drain_matching(
    tanks: &mut [Option<(Fluid, u32)>; 2],
    amount: u32,
    only: Option<Fluid>,
) -> Option<(Fluid, u32)> {
    let i = tanks
        .iter()
        .position(|t| t.is_some_and(|(f, _)| only.is_none_or(|want| want == f)))?;
    let (fluid, have) = tanks[i]?;
    let out = amount.min(have);
    tanks[i] = (have > out).then_some((fluid, have - out));
    (out > 0).then_some((fluid, out))
}

pub const INPUTS: std::ops::Range<usize> = 0..9;
pub const OUTPUTS: std::ops::Range<usize> = 9..18;
pub const SLOTS: usize = 18;

/// `HeatCondition`: a recipe's `heat_requirement`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HeatReq {
    #[default]
    None,
    Heated,
    Superheated,
}

impl HeatReq {
    pub fn read(name: Option<&str>) -> HeatReq {
        match name {
            Some("heated") => HeatReq::Heated,
            Some("superheated") => HeatReq::Superheated,
            _ => HeatReq::None,
        }
    }

    /// `testBlazeBurner` against the heat below the basin (`getHeatLevelOf`).
    pub fn met(self, heat: Heat) -> bool {
        match self {
            HeatReq::Superheated => heat == Heat::Seething,
            HeatReq::Heated => heat != Heat::None && heat != Heat::Smouldering,
            HeatReq::None => true,
        }
    }
}

/// A basin recipe: one item per ingredient, its results and time, and the heat it needs.
#[derive(Clone, Debug, PartialEq)]
pub struct Recipe {
    pub id: String,
    pub ingredients: Vec<Vec<String>>,
    pub results: Vec<Output>,
    pub time: i32,
    pub heat: HeatReq,
    /// Fluid ingredients and results (`FluidIngredient`, `FluidStack`).
    pub fluids: Vec<(Fluid, u32)>,
    pub fluid_results: Vec<(Fluid, u32)>,
}

/// What a basin recipe is applied with: the heat below, the item stack limits and crafting
/// remainders, and whether the basin faces a side (`acceptOutputs`: its item results then go to
/// the spout's buffer, not its outputs).
pub struct Context<'a> {
    pub heat: Heat,
    pub max: &'a dyn Fn(&str) -> u8,
    pub remainder: &'a dyn Fn(&str) -> Option<String>,
    pub spout: bool,
}

/// `BasinBlockEntity.fluidCapability`'s tanks in order: the outputs, then the inputs.
fn tank(tanks: &mut Tanks, i: usize) -> &mut Option<(Fluid, u32)> {
    if i < 2 {
        &mut tanks.output[i]
    } else {
        &mut tanks.input[i - 2]
    }
}

/// `BasinRecipe.apply` on the basin's 18 slots (inputs, then outputs) and its tanks: a simulated
/// pass, then (unless `test`) the real one. Each ingredient takes one item from the first slot
/// that fits, each fluid ingredient its amount from the first tank holding that much; the results
/// are rolled in the simulated pass, followed by the taken items' crafting remainders, and each
/// must fit on its own (`acceptOutputs`). The item results, on success.
///
/// The real pass changes the basin as it goes: when it fails part way (two ingredients counted
/// on one tank, results that only fit one at a time), what it took stays taken, as in Java.
pub fn apply(
    slots: &mut [Slot],
    tanks: &mut Tanks,
    recipe: &Recipe,
    cx: &Context,
    rng: &mut Rng,
    test: bool,
) -> Option<Vec<(String, u8)>> {
    if !recipe.heat.met(cx.heat) {
        return None;
    }
    let mut results = Vec::new();
    let mut fluid_results = Vec::new();
    for simulate in [true, false] {
        if !simulate && test {
            return Some(results);
        }
        let mut extracted = [0u32; SLOTS];
        for ingredient in &recipe.ingredients {
            let slot = (0..SLOTS).find(|&i| {
                slots[i].as_ref().is_some_and(|(item, n)| {
                    !(simulate && u32::from(*n) <= extracted[i]) && ingredient.contains(item)
                })
            })?;
            if !simulate && let Some((_, n)) = slots[slot].as_mut() {
                *n -= 1;
                if *n == 0 {
                    slots[slot] = None;
                }
            }
            extracted[slot] += 1;
        }
        let mut drained = [0u32; 4];
        for &(fluid, amount) in &recipe.fluids {
            let t = (0..4).find(|&t| {
                tank(tanks, t).is_some_and(|(f, n)| {
                    !(simulate && n <= drained[t]) && f == fluid && n >= amount
                })
            })?;
            if !simulate {
                let held = tank(tanks, t);
                let (f, n) = held.unwrap();
                *held = (n > amount).then_some((f, n - amount));
            }
            drained[t] += amount;
        }
        if simulate {
            results = crate::processing::roll(
                &crate::processing::Recipe {
                    id: recipe.id.clone(),
                    ingredients: Vec::new(),
                    results: recipe.results.clone(),
                    time: recipe.time,
                },
                rng,
            );
            fluid_results = recipe.fluid_results.clone();
            for (i, &n) in extracted.iter().enumerate() {
                let Some(left) = slots[i].as_ref().and_then(|(item, _)| (cx.remainder)(item))
                else {
                    continue;
                };
                results.extend((0..n).map(|_| (left.clone(), 1)));
            }
        }
        if !cx.spout {
            for (item, n) in &results {
                let max = (cx.max)(item);
                let left = if simulate {
                    insert_one_stack(&mut slots[OUTPUTS].to_vec(), item, *n, max)
                } else {
                    insert_one_stack(&mut slots[OUTPUTS], item, *n, max)
                };
                if left > 0 {
                    return None;
                }
            }
        }
        for &(fluid, amount) in &fluid_results {
            let filled = if simulate {
                fill(&mut tanks.output.clone(), fluid, amount)
            } else {
                fill(&mut tanks.output, fluid, amount)
            };
            if filled != amount {
                return None;
            }
        }
    }
    Some(results)
}

/// `ItemHandlerHelper.insertItemStacked` into a `BasinInventory`, whose `insertItem` refuses an
/// item another slot holds and puts a new one only in the first empty slot: one slot per item.
/// What did not fit.
pub fn insert_one_stack(slots: &mut [Slot], item: &str, count: u8, max: u8) -> u8 {
    let mut holding =
        (0..slots.len()).filter(|&i| slots[i].as_ref().is_some_and(|(x, _)| x == item));
    let at = match (holding.next(), holding.next()) {
        (Some(_), Some(_)) => return count,
        (Some(i), None) => i,
        (None, _) => match slots.iter().position(Option::is_none) {
            Some(i) => i,
            None => return count,
        },
    };
    insert_stacked(&mut slots[at..at + 1], item, count, max)
}

/// `getMatchingRecipes`: the recipes that would apply now, those with the most ingredients first.
pub fn matching(recipes: &[Recipe], slots: &[Slot], tanks: &Tanks, cx: &Context) -> Vec<usize> {
    let mut out: Vec<usize> = (0..recipes.len())
        .filter(|&i| {
            apply(
                &mut slots.to_vec(),
                &mut tanks.clone(),
                &recipes[i],
                cx,
                &mut Rng::new(1),
                true,
            )
            .is_some()
        })
        .collect();
    out.sort_by_key(|&i| std::cmp::Reverse(recipes[i].ingredients.len() + recipes[i].fluids.len()));
    out
}

/// `DeferralBehaviour`: its owner sees to what it was told of on its own turn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DeferralBehaviour {
    needs_update: bool,
}

impl DeferralBehaviour {
    /// `scheduleUpdate`.
    pub fn schedule_update(&mut self) {
        self.needs_update = true;
    }

    pub fn needs_update(&self) -> bool {
        self.needs_update
    }

    /// Whether an update is due, spending it: its owner's `tick` runs the callback now.
    pub fn take_update(&mut self) -> bool {
        std::mem::take(&mut self.needs_update)
    }
}

/// `BasinOperatingBlockEntity`: what a mechanical mixer or a mechanical press keeps for the
/// basin two blocks below it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BasinOperatingBlockEntity {
    /// `basinChecker`.
    pub basin_checker: DeferralBehaviour,
    /// `basinRemoved`: the basin was emptied or taken away; `onBasinRemoved` runs at the
    /// operator's next turn.
    pub basin_removed: bool,
    /// `currentRecipe`.
    pub current_recipe: Option<Recipe>,
}

/// The basins of a level and the operators over them.
#[derive(Default)]
pub struct Operators {
    /// `BasinBlockEntity.contentsChanged`, by basin, set since its last turn.
    pub contents_changed: HashSet<[i32; 3]>,
    /// Each operator's `BasinOperatingBlockEntity`, by the operator's own position.
    pub by_pos: HashMap<[i32; 3], BasinOperatingBlockEntity>,
}

/// What `BasinOperatingBlockEntity.tick` asks of the basin before the operator's own tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperatorAsk {
    /// The basin was emptied or taken away: `onBasinRemoved` ran, and the base's tick ends
    /// there. A subclass's own `tick` goes on after `super.tick()`: a mixer's
    /// [`CreateWorld::mixer_turn`] still runs, a press's cycle (a behaviour's) does not.
    Removed,
    /// Nothing: no update was due, or the operator is too slow, still, running or without a
    /// basin. An update is never held over.
    None,
    /// `updateBasin` got to the basin: whether its spout is free (`canContinueProcessing`) and
    /// a recipe fits is the glue's to say, to [`crate::world::CreateWorld::operator_start`].
    Basin,
}

/// The mixer's cycle (`MechanicalMixerBlockEntity.tick`): 20 ticks down, the processing time at
/// the bottom, 20 ticks up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mixer {
    pub running: bool,
    pub running_ticks: i32,
    /// `processingTicks`: 0 on a new mixer, whose first time at the bottom spends a tick going
    /// to −1; −1 again once the recipe was applied. A start leaves it, so a cycle resumed
    /// after a pause counts on from where it stopped.
    pub processing_ticks: i32,
}

/// What a tick of the mixer asks of the glue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MixStep {
    None,
    /// At the bottom, the time is up: apply the recipe (`applyBasinRecipe`).
    Apply,
    /// The cycle ended, and the basin is looked at at the mixer's next turn.
    Done,
}

/// `Mth.log2`: the floor of the base-2 logarithm.
fn log2(v: i32) -> i32 {
    31 - (v.max(1) as u32).leading_zeros() as i32
}

impl Mixer {
    /// `MechanicalMixerBlockEntity.startProcessingBasin`.
    pub fn start_processing_basin(&mut self) {
        if self.running && self.running_ticks <= 20 {
            return;
        }
        self.running = true;
        self.running_ticks = 0;
    }

    /// `continueWithPreviousRecipe`: the head stays down for the next recipe.
    pub fn continue_with_previous_recipe(&mut self) {
        self.running_ticks = 20;
    }

    /// One server tick at `speed` (rpm, signed); `time` the recipe's processing duration.
    pub fn tick(&mut self, speed: f32, time: i32) -> MixStep {
        // `isSpeedRequirementFulfilled` (`MechanicalMixerBlock`'s medium speed): too slow, the
        // head goes back up as when still.
        let fulfilled = speed.abs() >= MIXER_MIN_SPEED;
        if self.running_ticks >= 40 {
            self.running = false;
            self.running_ticks = 0;
            return MixStep::Done;
        }
        if !self.running {
            return MixStep::None;
        }
        let speed = speed.abs();
        let mut step = MixStep::None;
        if speed == 0.0 || !fulfilled {
            if self.running_ticks < 20 {
                self.running_ticks = 40 - self.running_ticks;
            } else if self.running_ticks == 20 {
                self.running_ticks += 1;
            }
        }
        if self.running_ticks == 20 {
            if self.processing_ticks < 0 {
                let recipe_speed = if time != 0 { time as f32 / 100.0 } else { 1.0 };
                self.processing_ticks =
                    (log2((512.0 / speed) as i32) * (recipe_speed * 15.0).ceil() as i32 + 1).max(1);
            } else {
                self.processing_ticks -= 1;
                if self.processing_ticks == 0 {
                    self.running_ticks += 1;
                    self.processing_ticks = -1;
                    step = MixStep::Apply;
                }
            }
        }
        if self.running_ticks != 20 {
            self.running_ticks += 1;
        }
        step
    }
}

/// `MechanicalMixerBlock.getMinimumRequiredSpeedLevel`: `MEDIUM`, Create's `mediumSpeed`
/// (30 rpm by default).
pub const MIXER_MIN_SPEED: f32 = 30.0;

/// `Iterate.horizontalDirections`.
const HORIZONTAL: [crate::kinetics::Dir; 4] = [
    crate::kinetics::Dir::South,
    crate::kinetics::Dir::West,
    crate::kinetics::Dir::North,
    crate::kinetics::Dir::East,
];

fn above(pos: [i32; 3], n: i32) -> [i32; 3] {
    [pos[0], pos[1] + n, pos[2]]
}

/// `getBasin`: the basin two below the operator at `pos`.
fn basin_of(world: &crate::world::CreateWorld, dim: usize, pos: [i32; 3]) -> Option<[i32; 3]> {
    let at = above(pos, -2);
    (world.block_at(dim, at) == Some(Block::Basin)).then_some(at)
}

/// `isRunning` of the operator at `pos`.
fn is_running(d: &Dimension, pos: [i32; 3]) -> bool {
    match d.mixers.get(&pos) {
        Some(m) => m.running,
        None => d.presses.get(&pos).is_some_and(|p| p.running),
    }
}

/// `startProcessingBasin` of the operator at `pos`.
fn start_processing_basin(d: &mut Dimension, pos: [i32; 3]) {
    if let Some(m) = d.mixers.get_mut(&pos) {
        m.start_processing_basin();
    } else if let Some(p) = d.presses.get_mut(&pos)
        && !(p.running && p.ticks <= crate::press::CYCLE / 2)
    {
        p.start(crate::press::Mode::Basin);
        d.press_changes.push(pos);
    }
}

/// `onBasinRemoved`.
fn on_basin_removed(d: &mut Dimension, pos: [i32; 3]) {
    if let Some(m) = d.mixers.get_mut(&pos) {
        if m.running {
            m.running_ticks = 40;
            m.running = false;
        }
    } else if let Some(p) = d.presses.get_mut(&pos) {
        p.running = false;
        p.ticks = 0;
    }
}

impl crate::world::CreateWorld {
    /// `BasinBlockEntity`'s `contentsChanged = true`: its operator is told at the basin's
    /// next turn.
    pub fn basin_contents_changed(&mut self, dim: usize, basin: [i32; 3]) {
        if self.block_at(dim, basin) == Some(Block::Basin) {
            self.dims[dim]
                .basin_operators
                .contents_changed
                .insert(basin);
        }
    }

    /// `BasinBlockEntity.onEmptied`: the operator two above finds its basin gone at its next
    /// turn.
    pub fn basin_emptied(&mut self, dim: usize, basin: [i32; 3]) {
        let operator = above(basin, 2);
        if let Some(op) = self.dims[dim].basin_operators.by_pos.get_mut(&operator) {
            op.basin_removed = true;
        }
    }

    /// `BasinBlockEntity.tick` at the basin's turn, after its spout: what changed in the basin
    /// since its last turn has its operator told (`basinChecker.scheduleUpdate`), and a basin
    /// spouting into this one flagged in its turn (`contentsChanged`).
    pub fn basin_turn(&mut self, dim: usize, basin: [i32; 3]) {
        if !self.dims[dim]
            .basin_operators
            .contents_changed
            .remove(&basin)
        {
            return;
        }
        let operator = above(basin, 2);
        if let Some(op) = self.dims[dim].basin_operators.by_pos.get_mut(&operator) {
            op.basin_checker.schedule_update();
        }
        for side in HORIZONTAL {
            let o = side.offset();
            let into_it = [basin[0] + o[0], basin[1] + 1, basin[2] + o[2]];
            let spouts_here = self
                .state_at(dim, into_it)
                .and_then(|s| self.table.get(s))
                .is_some_and(|(b, i)| b == Block::Basin && i.facing == Some(side.opposite()));
            if spouts_here {
                self.dims[dim]
                    .basin_operators
                    .contents_changed
                    .insert(into_it);
            }
        }
    }

    /// `BasinOperatingBlockEntity.tick` at the operator's turn, before its own: a basin emptied
    /// or taken away since its last turn stops it there (`Removed`); told of its basin since
    /// (`basinChecker`), it looks once (`updateBasin`), which reaches the basin only for an
    /// operator turning fast enough and idle, over a basin.
    pub fn operator_ask(&mut self, dim: usize, pos: [i32; 3]) -> OperatorAsk {
        let speed = self.speed(dim, pos).unwrap_or(0.0);
        let over_basin = basin_of(self, dim, pos).is_some();
        let d = &mut self.dims[dim];
        let Some(op) = d.basin_operators.by_pos.get_mut(&pos) else {
            return OperatorAsk::None;
        };
        if op.basin_removed {
            op.basin_removed = false;
            on_basin_removed(d, pos);
            return OperatorAsk::Removed;
        }
        if !op.basin_checker.take_update() {
            return OperatorAsk::None;
        }
        let mixer = d.mixers.contains_key(&pos);
        let fast_enough = !mixer || speed.abs() >= MIXER_MIN_SPEED;
        if fast_enough && speed != 0.0 && !is_running(d, pos) && over_basin {
            OperatorAsk::Basin
        } else {
            OperatorAsk::None
        }
    }

    /// `updateBasin`'s end: with the recipe that fits its basin (`getMatchingRecipes`), the
    /// operator starts a cycle on it. Whether it did.
    pub fn operator_start(&mut self, dim: usize, pos: [i32; 3], recipe: Option<Recipe>) -> bool {
        let Some(recipe) = recipe else {
            return false;
        };
        let d = &mut self.dims[dim];
        let Some(op) = d.basin_operators.by_pos.get_mut(&pos) else {
            return false;
        };
        op.current_recipe = Some(recipe);
        start_processing_basin(d, pos);
        true
    }

    /// `MechanicalMixerBlockEntity.tick` at the mixer's turn, after [`Self::operator_ask`]:
    /// its cycle's step, and with `MixStep::Apply` the recipe to apply (`currentRecipe`). None
    /// without a mixer there.
    pub fn mixer_turn(
        &mut self,
        dim: usize,
        pos: [i32; 3],
        speed: f32,
    ) -> Option<(MixStep, Option<Recipe>)> {
        let d = &mut self.dims[dim];
        let time = d
            .basin_operators
            .by_pos
            .get(&pos)
            .and_then(|o| o.current_recipe.as_ref())
            .map_or(0, |r| r.time);
        let step = d.mixers.get_mut(&pos)?.tick(speed, time);
        if step == MixStep::Done
            && let Some(op) = d.basin_operators.by_pos.get_mut(&pos)
        {
            op.basin_checker.schedule_update();
        }
        let recipe = (step == MixStep::Apply)
            .then(|| {
                d.basin_operators
                    .by_pos
                    .get(&pos)
                    .and_then(|o| o.current_recipe.clone())
            })
            .flatten();
        Some((step, recipe))
    }

    /// `applyBasinRecipe` once its recipe went into the basin: its contents changed
    /// (`notifyChangeOfContents`); `continued`: the mixer stays down for the next one
    /// (`continueWithPreviousRecipe`).
    pub fn recipe_applied(&mut self, dim: usize, machine: [i32; 3], continued: bool) {
        let d = &mut self.dims[dim];
        d.basin_operators
            .contents_changed
            .insert(above(machine, -2));
        if continued && let Some(m) = d.mixers.get_mut(&machine) {
            m.continue_with_previous_recipe();
        }
    }

    /// `onPressingCompleted` at the press's turn: over a basin whose recipe still fits and can
    /// go on, its next cycle starts now (`startProcessingBasin`); else `basinChecker` looks at
    /// the basin at its next turn.
    pub fn pressing_completed(&mut self, dim: usize, pos: [i32; 3], restart: bool) {
        let d = &mut self.dims[dim];
        if restart {
            start_processing_basin(d, pos);
        } else if let Some(op) = d.basin_operators.by_pos.get_mut(&pos) {
            op.basin_checker.schedule_update();
        }
    }
}

/// The mixer's `runningTicks` a client works out `elapsed` ticks after it was told `told`, as
/// the client's own `tick` runs it on: down to the bottom (20), where it waits for the server,
/// then up again to 40.
pub fn shown_ticks(told: i32, elapsed: i64) -> i32 {
    let elapsed = elapsed.clamp(0, 40) as i32;
    match told {
        t if t < 20 => (t + elapsed).min(20),
        20 => 20,
        t => (t + elapsed).min(40),
    }
}

/// `MechanicalMixerBlockEntity.getRenderedHeadOffset`: how far the head is lowered, in blocks,
/// at `ticks` of a running cycle and `partial` of the frame (7/16 at rest).
pub fn head_offset(running: bool, ticks: i32, partial: f32) -> f32 {
    let ease = |num: f32| (2.0 - (num * std::f32::consts::PI).cos()) / 2.0 - 0.5;
    let offset = if !running {
        0.0
    } else if ticks < 20 {
        ease((ticks as f32 + partial) / 20.0)
    } else if ticks == 20 {
        1.0
    } else {
        ease(((40 - ticks) as f32 - partial) / 20.0)
    };
    offset + 7.0 / 16.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shown_head_goes_down_waits_and_comes_up() {
        assert_eq!(shown_ticks(0, 5), 5);
        assert_eq!(shown_ticks(0, 30), 20, "it waits at the bottom");
        assert_eq!(shown_ticks(20, 100), 20);
        assert_eq!(shown_ticks(21, 10), 31);
        assert_eq!(shown_ticks(21, 30), 40);
        assert_eq!(head_offset(false, 0, 0.0), 7.0 / 16.0);
        assert_eq!(head_offset(true, 20, 0.5), 1.0 + 7.0 / 16.0);
        assert!((head_offset(true, 0, 0.0) - 7.0 / 16.0).abs() < 1e-6);
    }

    fn alloy() -> Recipe {
        Recipe {
            id: "create:mixing/andesite_alloy".into(),
            ingredients: vec![
                vec!["minecraft:andesite".into()],
                vec!["minecraft:iron_nugget".into(), "create:zinc_nugget".into()],
            ],
            results: vec![Output {
                item: "create:andesite_alloy".into(),
                count: 1,
                chance: 1.0,
            }],
            time: 0,
            heat: HeatReq::None,
            fluids: vec![],
            fluid_results: vec![],
        }
    }

    fn cx(heat: Heat) -> Context<'static> {
        Context {
            heat,
            max: &|_| 64,
            remainder: &|_| None,
            spout: false,
        }
    }

    fn run(slots: &mut [Slot], tanks: &mut Tanks, r: &Recipe, heat: Heat, test: bool) -> bool {
        apply(slots, tanks, r, &cx(heat), &mut Rng::new(1), test).is_some()
    }

    #[test]
    fn a_recipe_takes_one_of_each_and_fills_the_outputs() {
        let mut slots: Vec<Slot> = vec![None; SLOTS];
        slots[0] = Some(("minecraft:andesite".into(), 2));
        slots[1] = Some(("minecraft:iron_nugget".into(), 1));
        let r = alloy();
        assert_eq!(
            matching(
                std::slice::from_ref(&r),
                &slots,
                &Tanks::default(),
                &cx(Heat::None)
            ),
            vec![0]
        );
        assert!(run(
            &mut slots,
            &mut Tanks::default(),
            &r,
            Heat::None,
            false
        ));
        assert_eq!(slots[0], Some(("minecraft:andesite".into(), 1)));
        assert_eq!(slots[1], None);
        assert_eq!(slots[9], Some(("create:andesite_alloy".into(), 1)));
        assert!(
            !run(&mut slots, &mut Tanks::default(), &r, Heat::None, true),
            "no nugget left"
        );
        let brass = Recipe {
            heat: HeatReq::Heated,
            ..alloy()
        };
        slots[1] = Some(("minecraft:iron_nugget".into(), 1));
        assert!(!run(
            &mut slots,
            &mut Tanks::default(),
            &brass,
            Heat::Smouldering,
            true
        ));
        assert!(run(
            &mut slots,
            &mut Tanks::default(),
            &brass,
            Heat::Kindled,
            true
        ));
    }

    #[test]
    fn ingredients_come_from_the_outputs_too() {
        let mut slots: Vec<Slot> = vec![None; SLOTS];
        slots[9] = Some(("minecraft:andesite".into(), 1));
        slots[10] = Some(("create:zinc_nugget".into(), 3));
        assert!(run(
            &mut slots,
            &mut Tanks::default(),
            &alloy(),
            Heat::None,
            false
        ));
        assert_eq!(slots[9], Some(("create:andesite_alloy".into(), 1)));
        assert_eq!(slots[10], Some(("create:zinc_nugget".into(), 2)));
    }

    #[test]
    fn a_result_goes_only_onto_the_one_stack_of_its_item() {
        let mut slots: Vec<Slot> = vec![None; SLOTS];
        slots[0] = Some(("minecraft:andesite".into(), 1));
        slots[1] = Some(("minecraft:iron_nugget".into(), 1));
        slots[9] = Some(("create:andesite_alloy".into(), 64));
        let before = slots.clone();
        assert!(!run(
            &mut slots,
            &mut Tanks::default(),
            &alloy(),
            Heat::None,
            false
        ));
        assert_eq!(slots, before);
        let mut outputs: Vec<Slot> = vec![None; 9];
        outputs[1] = Some(("x".into(), 63));
        assert_eq!(insert_one_stack(&mut outputs, "x", 3, 64), 2);
        assert_eq!(outputs[1], Some(("x".into(), 64)));
        assert_eq!(outputs[0], None);
        outputs[4] = Some(("x".into(), 1));
        assert_eq!(insert_one_stack(&mut outputs, "x", 1, 64), 1);
    }

    #[test]
    fn crafting_remainders_join_the_results() {
        let honey = Recipe {
            id: "crafting:minecraft:honey_bottle:2".into(),
            ingredients: vec![vec!["minecraft:honey_bottle".into()]; 4],
            results: vec![Output {
                item: "minecraft:honey_block".into(),
                count: 1,
                chance: 1.0,
            }],
            time: 0,
            heat: HeatReq::None,
            fluids: vec![],
            fluid_results: vec![],
        };
        let mut slots: Vec<Slot> = vec![None; SLOTS];
        slots[0] = Some(("minecraft:honey_bottle".into(), 5));
        let cx = Context {
            remainder: &|i| {
                (i == "minecraft:honey_bottle").then(|| "minecraft:glass_bottle".into())
            },
            ..cx(Heat::None)
        };
        let made = apply(
            &mut slots,
            &mut Tanks::default(),
            &honey,
            &cx,
            &mut Rng::new(1),
            false,
        );
        assert_eq!(made.map(|m| m.len()), Some(5));
        assert_eq!(slots[0], Some(("minecraft:honey_bottle".into(), 1)));
        assert_eq!(slots[9], Some(("minecraft:honey_block".into(), 1)));
        assert_eq!(slots[10], Some(("minecraft:glass_bottle".into(), 4)));
    }

    #[test]
    fn fluid_ingredients_drain_the_outputs_first_and_one_tank_each() {
        let twice = Recipe {
            id: "test:twice".into(),
            ingredients: vec![vec!["minecraft:sugar".into()]],
            results: vec![],
            time: 0,
            heat: HeatReq::None,
            fluids: vec![(Fluid::Water, 600), (Fluid::Water, 600)],
            fluid_results: vec![],
        };
        let mut slots: Vec<Slot> = vec![None; SLOTS];
        slots[0] = Some(("minecraft:sugar".into(), 2));
        let mut tanks = Tanks {
            input: [Some((Fluid::Water, 1000)), None],
            output: [Some((Fluid::Water, 600)), None],
        };
        assert!(run(&mut slots, &mut tanks, &twice, Heat::None, false));
        assert_eq!(tanks.output[0], None);
        assert_eq!(tanks.input[0], Some((Fluid::Water, 400)));
        // Both counted on the one tank: the simulated pass passes, the real one fails part way.
        tanks.input[0] = Some((Fluid::Water, 1000));
        assert!(run(
            &mut slots.clone(),
            &mut tanks.clone(),
            &twice,
            Heat::None,
            true
        ));
        assert!(!run(&mut slots, &mut tanks, &twice, Heat::None, false));
        assert_eq!(slots[0], None);
        assert_eq!(tanks.input[0], Some((Fluid::Water, 400)));
    }

    #[test]
    fn chocolate_takes_milk_from_a_tank_and_fills_an_output() {
        // data/create/recipe/mixing/chocolate.json: sugar, cocoa beans, 250 milk -> 250 chocolate.
        let r = Recipe {
            id: "create:mixing/chocolate".into(),
            ingredients: vec![
                vec!["minecraft:sugar".into()],
                vec!["minecraft:cocoa_beans".into()],
            ],
            results: vec![],
            time: 0,
            heat: HeatReq::Heated,
            fluids: vec![(Fluid::Milk, 250)],
            fluid_results: vec![(Fluid::Chocolate, 250)],
        };
        let mut slots: Vec<Slot> = vec![None; SLOTS];
        slots[0] = Some(("minecraft:sugar".into(), 2));
        slots[1] = Some(("minecraft:cocoa_beans".into(), 2));
        let mut tanks = Tanks::default();
        assert!(
            !run(
                &mut slots.clone(),
                &mut tanks.clone(),
                &r,
                Heat::Kindled,
                true
            ),
            "no milk"
        );
        assert_eq!(fill(&mut tanks.input, Fluid::Milk, 400), 400);
        assert_eq!(
            matching(std::slice::from_ref(&r), &slots, &tanks, &cx(Heat::Kindled)),
            vec![0]
        );
        assert!(run(&mut slots, &mut tanks, &r, Heat::Kindled, false));
        assert_eq!(tanks.input[0], Some((Fluid::Milk, 150)));
        assert_eq!(tanks.output[0], Some((Fluid::Chocolate, 250)));
        assert!(
            !run(
                &mut slots.clone(),
                &mut tanks.clone(),
                &r,
                Heat::Kindled,
                true
            ),
            "150 left"
        );
        assert_eq!(
            drain(&mut tanks.output, 1000),
            Some((Fluid::Chocolate, 250))
        );
    }

    #[test]
    fn the_mixer_goes_down_mixes_and_comes_back() {
        let mut m = Mixer::default();
        m.start_processing_basin();
        let mut applied_at = None;
        for t in 0..200 {
            match m.tick(64.0, 0) {
                MixStep::Apply => applied_at = Some(t),
                MixStep::Done => {
                    // 64 rpm: log2(8) × 15 + 1 = 46 ticks at the bottom, after the tick a new
                    // mixer's count (0) takes to go below zero.
                    assert_eq!(applied_at, Some(20 + 1 + 46));
                    // The applying tick moves on twice (21, then 22): 19 more ticks up.
                    assert_eq!(t, applied_at.unwrap() + 19);
                    return;
                }
                MixStep::None => {}
            }
        }
        panic!("the cycle never ended");
    }

    /// `processing-basin-mixer-timing`: a new mixer's first time at the bottom takes one tick
    /// more (`processingTicks` starts at 0), and a mixer stopped mid-cycle and started again
    /// counts on from the ticks it had left (`startProcessingBasin` leaves it).
    #[test]
    fn a_mixer_started_again_keeps_its_processing_ticks() {
        let mut m = Mixer::default();
        m.start_processing_basin();
        for _ in 0..20 {
            assert_eq!(m.tick(256.0, 0), MixStep::None);
        }
        assert_eq!((m.running_ticks, m.processing_ticks), (20, 0));
        assert_eq!(m.tick(256.0, 0), MixStep::None);
        assert_eq!(m.processing_ticks, -1);
        m.tick(256.0, 0);
        assert_eq!(m.processing_ticks, 16);
        for _ in 0..10 {
            assert_eq!(m.tick(256.0, 0), MixStep::None);
        }
        assert_eq!((m.running_ticks, m.processing_ticks), (20, 6));
        // Too slow: the head leaves the bottom and climbs back to rest.
        assert_eq!(m.tick(16.0, 0), MixStep::None);
        assert_eq!(m.running_ticks, 22);
        for _ in 0..18 {
            assert_eq!(m.tick(16.0, 0), MixStep::None);
        }
        assert_eq!(m.running_ticks, 40);
        assert_eq!(m.tick(256.0, 0), MixStep::Done);
        assert_eq!(
            (m.running, m.running_ticks, m.processing_ticks),
            (false, 0, 6)
        );
        m.start_processing_basin();
        for _ in 0..25 {
            assert_eq!(m.tick(256.0, 0), MixStep::None);
        }
        assert_eq!((m.running_ticks, m.processing_ticks), (20, 1));
        assert_eq!(m.tick(256.0, 0), MixStep::Apply);
        assert_eq!(m.processing_ticks, -1);
    }
}

#[cfg(test)]
mod flow_source_tests {
    use super::*;
    #[test]
    fn source_filter_can_extract_the_second_output_fluid() {
        let mut outputs = [Some((Fluid::Water, 500)), Some((Fluid::Lava, 250))];
        assert_eq!(
            drain_matching(&mut outputs, 16, Some(Fluid::Lava)),
            Some((Fluid::Lava, 16))
        );
        assert_eq!(
            outputs,
            [Some((Fluid::Water, 500)), Some((Fluid::Lava, 234))]
        );
    }
}

#[cfg(test)]
mod operator_tests {
    use super::*;
    use crate::parts::Settings;
    use crate::press::{Mode, Tick};
    use crate::states::TEST_LOCK;
    use crate::world::{CreateWorld, StateTable, TransportTick};
    use std::sync::MutexGuard;

    type Pos = [i32; 3];
    type Props = Vec<(&'static str, &'static str)>;

    const BASIN: Pos = [0, 6, 0];
    const OPERATOR: Pos = [0, 8, 0];

    fn snow_block() -> Recipe {
        Recipe {
            id: "minecraft:snow_block".into(),
            ingredients: vec![vec!["minecraft:snowball".into()]; 4],
            results: vec![Output {
                item: "minecraft:snow_block".into(),
                count: 1,
                chance: 1.0,
            }],
            time: 0,
            heat: HeatReq::None,
            fluids: vec![],
            fluid_results: vec![],
        }
    }

    fn alloy() -> Recipe {
        Recipe {
            id: "create:mixing/andesite_alloy".into(),
            ingredients: vec![
                vec!["minecraft:andesite".into()],
                vec!["minecraft:iron_nugget".into()],
            ],
            results: vec![Output {
                item: "create:andesite_alloy".into(),
                count: 1,
                chance: 1.0,
            }],
            time: 0,
            heat: HeatReq::None,
            fluids: vec![],
            fluid_results: vec![],
        }
    }

    fn basin_state() -> (&'static str, Props) {
        ("create:basin", vec![("facing", "down")])
    }

    /// A world whose Create states are `names`, in that order at `2..`, under the test lock.
    /// `clear_range` before the last of its values goes.
    fn world(names: &[(&str, Props)]) -> (MutexGuard<'static, ()>, CreateWorld) {
        let guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        crate::states::set_range(2, names.len() as u32);
        let table = StateTable::new(2, names.iter().map(|(n, p)| (*n, p.iter().copied())));
        (guard, CreateWorld::new(table))
    }

    /// A world under the states' test lock, which goes with the range cleared.
    struct Locked {
        w: CreateWorld,
        motor: (Pos, u32),
        basin: u32,
        _guard: MutexGuard<'static, ()>,
    }

    impl Drop for Locked {
        fn drop(&mut self) {
            crate::states::clear_range();
        }
    }

    /// A press or a mixer turned at 256 rpm two above a basin facing down, its blocks placed
    /// before the basin or after it, then two ticks with nothing in the basin: the looks its
    /// own first speed report and the basin's arrival earned are spent. The motor's position
    /// and state, the basin's state.
    fn over_basin(mixer: bool, basin_first: bool) -> Locked {
        let machine: Vec<(Pos, &str, Props)> = if mixer {
            vec![
                ([-1, 7, 0], "create:creative_motor", vec![("facing", "up")]),
                (
                    [-1, 8, 0],
                    "create:cogwheel",
                    vec![("axis", "y"), ("waterlogged", "false")],
                ),
                (OPERATOR, "create:mechanical_mixer", vec![]),
            ]
        } else {
            vec![
                (
                    [0, 8, -1],
                    "create:creative_motor",
                    vec![("facing", "south")],
                ),
                (
                    OPERATOR,
                    "create:mechanical_press",
                    vec![("facing", "north")],
                ),
            ]
        };
        let (name, props) = basin_state();
        let n = machine.len();
        let blocks: Vec<(Pos, &str, Props)> = if basin_first {
            std::iter::once((BASIN, name, props))
                .chain(machine)
                .collect()
        } else {
            machine
                .into_iter()
                .chain(std::iter::once((BASIN, name, props)))
                .collect()
        };
        let names: Vec<(&str, Props)> = blocks.iter().map(|(_, n, p)| (*n, p.clone())).collect();
        let (guard, mut w) = world(&names);
        for (i, (p, _, _)) in blocks.iter().enumerate() {
            w.block_changed(0, *p, 0, 2 + i as u32);
        }
        let motor_index = usize::from(basin_first);
        let motor = (blocks[motor_index].0, 2 + motor_index as u32);
        w.set_settings(
            0,
            motor.0,
            motor.1,
            Settings {
                motor_rpm: 256.0,
                ..Settings::default()
            },
        );
        w.tick(0, &mut Vec::new());
        assert_eq!(w.speed(0, OPERATOR).unwrap().abs(), 256.0);
        let mut none = || None;
        let mut never = |_: &mut CreateWorld| false;
        for _ in 0..2 {
            level_tick(&mut w, &mut none, &mut never);
        }
        assert!(!told(&w, OPERATOR));
        assert!(w.dims[0].basin_operators.contents_changed.is_empty());
        let basin = 2 + if basin_first { 0 } else { n as u32 };
        Locked {
            w,
            motor,
            basin,
            _guard: guard,
        }
    }

    fn state_of(w: &CreateWorld, pos: Pos) -> (bool, i32) {
        match w.dims[0].mixers.get(&pos) {
            Some(m) => (m.running, m.running_ticks),
            None => {
                let p = w.dims[0].presses[&pos];
                (p.running, p.ticks)
            }
        }
    }

    /// What the operator's turn did before its own tick.
    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Turn {
        Removed,
        Started,
        Tick,
    }

    /// One level tick as the glue runs it for these blocks: each basin's, mixer's and press's
    /// turn, in the walk's order. `matching` answers `getMatchingRecipes`, `restart` the press's
    /// `onPressingCompleted`. The operator's turns, and its `running` and `runningTicks`.
    fn level_tick(
        w: &mut CreateWorld,
        matching: &mut dyn FnMut() -> Option<Recipe>,
        restart: &mut dyn FnMut(&mut CreateWorld) -> bool,
    ) -> (Vec<Turn>, (bool, i32)) {
        let mut turns = Vec::new();
        let mut state = (false, 0);
        let mut walk = w.transport_ticks(0, 0);
        while let Some(turn) = walk.next(w) {
            match turn {
                TransportTick::Basin(p) => w.basin_turn(0, p),
                TransportTick::Mixer(p) | TransportTick::Press(p) => {
                    let turn = match w.operator_ask(0, p) {
                        OperatorAsk::Removed => Turn::Removed,
                        OperatorAsk::None => Turn::Tick,
                        OperatorAsk::Basin => {
                            if w.operator_start(0, p, matching()) {
                                Turn::Started
                            } else {
                                Turn::Tick
                            }
                        }
                    };
                    turns.push(turn);
                    let speed = w.speed(0, p).unwrap_or(0.0);
                    if let Some((step, _recipe)) = w.mixer_turn(0, p, speed) {
                        if step == MixStep::Apply {
                            w.recipe_applied(0, p, false);
                        }
                    } else if turn != Turn::Removed {
                        let (mode, tick) = w.tick_press(0, p).unwrap();
                        if tick == Tick::Done {
                            let again = mode == Mode::Basin && restart(w);
                            w.pressing_completed(0, p, again);
                        }
                    }
                    state = state_of(w, p);
                }
                _ => {}
            }
        }
        (turns, state)
    }

    fn told(w: &CreateWorld, pos: Pos) -> bool {
        w.dims[0].basin_operators.by_pos[&pos]
            .basin_checker
            .needs_update()
    }

    fn recipe_of(w: &CreateWorld, pos: Pos) -> Option<String> {
        w.dims[0].basin_operators.by_pos[&pos]
            .current_recipe
            .as_ref()
            .map(|r| r.id.clone())
    }

    #[test]
    fn a_deferral_spends_an_update_on_one_look() {
        let mut d = DeferralBehaviour::default();
        assert!(!d.take_update(), "nothing was scheduled");
        d.schedule_update();
        d.schedule_update();
        assert!(d.take_update());
        assert!(!d.needs_update(), "one look, however often told");
        assert!(!d.take_update());
    }

    /// `processing-basin-press-start`: 8 snowballs put in at tick 12. A press at 256 rpm (30
    /// cycle ticks a tick) placed before its basin starts at its own turn, the tick after the
    /// basin was told; placed after it, in the tick the basin was told.
    #[test]
    fn an_operator_starts_at_its_own_turn() {
        for (basin_first, ticks) in [
            (false, [(false, 0), (true, 30), (true, 60)]),
            (true, [(true, 30), (true, 60), (true, 90)]),
        ] {
            let mut f = over_basin(false, basin_first);
            let mut snow = || Some(snow_block());
            let mut never = |_: &mut CreateWorld| false;
            f.w.basin_contents_changed(0, BASIN);
            for expected in ticks {
                assert_eq!(level_tick(&mut f.w, &mut snow, &mut never).1, expected);
            }
            assert_eq!(f.w.dims[0].presses[&OPERATOR].mode, Mode::Basin);
            assert_eq!(
                recipe_of(&f.w, OPERATOR).as_deref(),
                Some("minecraft:snow_block")
            );
        }
    }

    /// A told operator looks once, at its next turn. With no recipe then, the look is spent
    /// (`updateBasin` answers true whatever it finds): a recipe that fits later starts nothing
    /// until the basin is told again.
    #[test]
    fn a_told_operator_looks_once_at_its_next_turn() {
        let mut f = over_basin(true, false);
        let mut none = || None;
        let mut never = |_: &mut CreateWorld| false;
        let mut alloy = || Some(alloy());
        f.w.basin_contents_changed(0, BASIN);
        assert_eq!(
            level_tick(&mut f.w, &mut none, &mut never),
            (vec![Turn::Tick], (false, 0))
        );
        assert!(told(&f.w, OPERATOR), "the basin's turn follows the mixer's");
        assert_eq!(level_tick(&mut f.w, &mut none, &mut never).1, (false, 0));
        assert!(!told(&f.w, OPERATOR), "the look is spent");
        assert_eq!(level_tick(&mut f.w, &mut alloy, &mut never).1, (false, 0));
        f.w.basin_contents_changed(0, BASIN);
        assert_eq!(level_tick(&mut f.w, &mut alloy, &mut never).1, (false, 0));
        assert_eq!(
            level_tick(&mut f.w, &mut alloy, &mut never),
            (vec![Turn::Started], (true, 1))
        );
        assert_eq!(level_tick(&mut f.w, &mut alloy, &mut never).1, (true, 2));
        assert_eq!(
            recipe_of(&f.w, OPERATOR).as_deref(),
            Some("create:mixing/andesite_alloy")
        );
    }

    /// `processing-basin-emptied`, `processing-basin-press-removed`: a basin emptied by hand or
    /// taken away stops its operator at its next turn (`onBasinRemoved`). A running mixer 31
    /// ticks up at checkpoint 60 rests at 61: its own `tick` goes on after the base's
    /// (`runningTicks >= 40`) and has it look again; a press's cycle, the base's, stops.
    #[test]
    fn an_emptied_or_taken_basin_stops_its_operator_at_its_next_turn() {
        let mut none = || None;
        let mut never = |_: &mut CreateWorld| false;
        let mut alloy = || Some(alloy());
        {
            let mut f = over_basin(true, false);
            let w = &mut f.w;
            w.basin_contents_changed(0, BASIN);
            level_tick(w, &mut alloy, &mut never);
            level_tick(w, &mut alloy, &mut never);
            while w.dims[0].mixers[&OPERATOR].running_ticks < 31 {
                level_tick(w, &mut none, &mut never);
            }
            assert_eq!(w.dims[0].mixers[&OPERATOR].processing_ticks, -1);
            w.basin_emptied(0, BASIN);
            assert_eq!(
                level_tick(w, &mut alloy, &mut never),
                (vec![Turn::Removed], (false, 0))
            );
            assert!(told(w, OPERATOR), "resting, it looks again");
            assert_eq!(level_tick(w, &mut none, &mut never).1, (false, 0));
            assert!(!told(w, OPERATOR));
        }
        {
            let mut f = over_basin(true, false);
            let w = &mut f.w;
            w.basin_contents_changed(0, BASIN);
            level_tick(w, &mut alloy, &mut never);
            level_tick(w, &mut alloy, &mut never);
            assert_eq!(w.dims[0].mixers[&OPERATOR].running_ticks, 1);
            w.block_changed(0, BASIN, f.basin, 0);
            assert_eq!(
                level_tick(w, &mut alloy, &mut never),
                (vec![Turn::Removed], (false, 0))
            );
        }
        let mut f = over_basin(false, true);
        let w = &mut f.w;
        let mut snow = || Some(snow_block());
        w.basin_contents_changed(0, BASIN);
        level_tick(w, &mut snow, &mut never);
        assert_eq!(level_tick(w, &mut snow, &mut never).1, (true, 60));
        w.block_changed(0, BASIN, f.basin, 0);
        assert_eq!(
            level_tick(w, &mut snow, &mut never),
            (vec![Turn::Removed], (false, 0))
        );
    }

    /// `processing-basin-press-restart`: a press at 256 rpm whose recipe still fits starts its
    /// next cycle in the tick its last one ends, back at 0; one whose recipe no longer fits
    /// rests there and looks at its basin at its next turn.
    #[test]
    fn a_press_restarts_in_the_tick_its_cycle_ends() {
        let mut snow = || Some(snow_block());
        let mut again = |_: &mut CreateWorld| true;
        let mut never = |_: &mut CreateWorld| false;
        {
            let mut f = over_basin(false, true);
            let w = &mut f.w;
            w.basin_contents_changed(0, BASIN);
            assert_eq!(level_tick(w, &mut snow, &mut again).1, (true, 30));
            let cycle: Vec<(bool, i32)> = (0..9)
                .map(|_| level_tick(w, &mut snow, &mut again).1)
                .collect();
            assert_eq!(
                cycle,
                [
                    (true, 60),
                    (true, 90),
                    (true, 120),
                    (true, 150),
                    (true, 180),
                    (true, 210),
                    (true, 240),
                    (true, 270),
                    (true, 0),
                ]
            );
            assert!(!told(w, OPERATOR));
            assert_eq!(level_tick(w, &mut snow, &mut again).1, (true, 30));
        }
        let mut f = over_basin(false, true);
        let w = &mut f.w;
        w.basin_contents_changed(0, BASIN);
        level_tick(w, &mut snow, &mut never);
        let mut last = (true, 0);
        for _ in 0..9 {
            last = level_tick(w, &mut snow, &mut never).1;
        }
        assert_eq!(last, (false, 270));
        assert!(told(w, OPERATOR), "it rests and looks again");
        assert_eq!(
            level_tick(w, &mut snow, &mut never),
            (vec![Turn::Started], (true, 30))
        );
    }

    /// `processing-basin-continue`: once the recipe went in the basin is told, and a mixer
    /// whose spout was free before and whose recipe still fits stays down
    /// (`continueWithPreviousRecipe`).
    #[test]
    fn a_mixer_goes_on_only_from_what_it_is_told() {
        let mut f = over_basin(true, false);
        let w = &mut f.w;
        w.dims[0].mixers.insert(
            OPERATOR,
            Mixer {
                running: true,
                running_ticks: 21,
                processing_ticks: -1,
            },
        );
        w.recipe_applied(0, OPERATOR, false);
        assert_eq!(w.dims[0].mixers[&OPERATOR].running_ticks, 21);
        assert_eq!(w.dims[0].basin_operators.contents_changed, [BASIN].into());
        w.dims[0].basin_operators.contents_changed.clear();
        w.recipe_applied(0, OPERATOR, true);
        assert_eq!(w.dims[0].mixers[&OPERATOR].running_ticks, 20);
        assert_eq!(w.dims[0].basin_operators.contents_changed, [BASIN].into());
    }

    /// `processing-basin-heater-lit`: any new state of a burner has the basin above it told
    /// (`BlazeBurnerBlock.onPlace`).
    #[test]
    fn a_burner_s_new_state_tells_the_basin_above() {
        let burner = |heat: &'static str| {
            (
                "create:blaze_burner",
                vec![("blaze", heat), ("facing", "north")],
            )
        };
        let (guard, mut w) = world(&[basin_state(), burner("smouldering"), burner("seething")]);
        let (smouldering, seething) = (3, 4);
        w.block_changed(0, BASIN, 0, 2);
        w.block_changed(0, [0, 5, 0], 0, smouldering);
        assert_eq!(w.dims[0].basin_operators.contents_changed, [BASIN].into());
        w.basin_turn(0, BASIN);
        assert!(w.dims[0].basin_operators.contents_changed.is_empty());
        w.block_changed(0, [0, 5, 0], smouldering, seething);
        assert_eq!(w.dims[0].basin_operators.contents_changed, [BASIN].into());
        drop(w);
        crate::states::clear_range();
        drop(guard);
        let (guard, mut w) = world(&[burner("seething")]);
        w.block_changed(0, [5, 5, 5], 0, 2);
        assert!(w.dims[0].basin_operators.contents_changed.is_empty());
        drop(w);
        crate::states::clear_range();
        drop(guard);
    }

    /// A basin whose contents changed has the basin spouting into it flagged in its turn
    /// (`BasinBlockEntity.tick`).
    #[test]
    fn a_basin_tells_the_one_spouting_into_it() {
        let (guard, mut w) = world(&[basin_state(), ("create:basin", vec![("facing", "west")])]);
        let (below, beside, past) = ([0, 6, 0], [1, 7, 0], [-1, 7, 0]);
        w.block_changed(0, below, 0, 2);
        w.block_changed(0, beside, 0, 3);
        w.block_changed(0, past, 0, 3);
        let flagged = |w: &CreateWorld| w.dims[0].basin_operators.contents_changed.clone();
        assert_eq!(flagged(&w), HashSet::from([below, beside, past]));
        for p in [below, beside, past] {
            w.basin_turn(0, p);
        }
        assert!(flagged(&w).is_empty());
        w.basin_contents_changed(0, below);
        w.basin_turn(0, below);
        assert_eq!(
            flagged(&w),
            HashSet::from([beside]),
            "the one spouting into it, not the one beside"
        );
        w.basin_turn(0, below);
        assert_eq!(
            flagged(&w),
            HashSet::from([beside]),
            "nothing changed since"
        );
        drop(w);
        crate::states::clear_range();
        drop(guard);
    }

    /// Basins, mixers and presses take their turns in the registration order they share with
    /// depots and belts.
    #[test]
    fn basins_mixers_and_presses_take_the_turns_in_registration_order() {
        let (guard, mut w) = world(&[
            ("create:mechanical_press", vec![("facing", "north")]),
            ("create:depot", vec![("waterlogged", "false")]),
            basin_state(),
            ("create:mechanical_mixer", vec![]),
        ]);
        let (press, depot, basin, mixer) = ([0, 8, 0], [1, 5, 0], [0, 6, 0], [0, 8, 1]);
        for (i, p) in [press, depot, basin, mixer].into_iter().enumerate() {
            w.block_changed(0, p, 0, 2 + i as u32);
        }
        let mut turns = Vec::new();
        let mut walk = w.transport_ticks(0, 0);
        while let Some(t) = walk.next(&mut w) {
            turns.push(t);
        }
        assert_eq!(
            turns,
            [
                TransportTick::Press(press),
                TransportTick::Depot(depot),
                TransportTick::Basin(basin),
                TransportTick::Mixer(mixer),
            ]
        );
        drop(w);
        crate::states::clear_range();
        drop(guard);
    }

    /// `BasinOperatingBlockEntity.onSpeedChanged`: a speed change has the operator look at its
    /// basin; too slow for a mixer (`isSpeedRequirementFulfilled`), the look starts nothing and
    /// is spent.
    #[test]
    fn a_speed_change_has_the_operator_look_at_its_basin() {
        let mut f = over_basin(true, false);
        let w = &mut f.w;
        w.dims[0].basin_operators.contents_changed.clear();
        w.basin_turn(0, BASIN);
        w.set_settings(
            0,
            f.motor.0,
            f.motor.1,
            Settings {
                motor_rpm: 16.0,
                ..Settings::default()
            },
        );
        w.tick(0, &mut Vec::new());
        assert!(told(w, OPERATOR));
        let mut alloy = || Some(alloy());
        let mut never = |_: &mut CreateWorld| false;
        assert_eq!(
            level_tick(w, &mut alloy, &mut never),
            (vec![Turn::Tick], (false, 0))
        );
        assert!(!told(w, OPERATOR));
    }
}
