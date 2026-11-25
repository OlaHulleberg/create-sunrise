//! Create's basin and the mechanical mixer over it (`BasinBlockEntity`, `BasinRecipe`,
//! `BasinOperatingBlockEntity`, `MechanicalMixerBlockEntity`, `HeatCondition`): items in the
//! basin's nine input slots, a recipe whose ingredients they hold (and whose heat the burner
//! below gives) is mixed in a cycle of the mixer's head, its results going to the nine outputs.

use crate::fan::Heat;
use crate::processing::{Output, Rng, Slot, insert_stacked};
use crate::tank::Fluid;
use std::collections::{BTreeMap, BTreeSet, HashMap};

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

/// `BasinInventory.insertItem`: whether `item` may go into `slot`. Only an input slot takes
/// it, none of the other inputs may already hold the same stack (`same`), and an empty slot
/// takes it only when it is the first empty input.
pub fn admits_slot<T>(
    slots: &[Option<T>],
    slot: usize,
    item: &T,
    same: impl Fn(&T, &T) -> bool,
) -> bool {
    if !INPUTS.contains(&slot) {
        return false;
    }
    let mut first_free = None;
    for i in INPUTS {
        let cur = slots.get(i).and_then(Option::as_ref);
        if i != slot && cur.is_some_and(|s| same(item, s)) {
            return false;
        }
        if cur.is_none() && first_free.is_none() {
            first_free = Some(i);
        }
    }
    slots.get(slot).and_then(Option::as_ref).is_some() || first_free == Some(slot)
}

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

/// `IntArrayTrie`: values stored under sorted `u32` keys, looked up for every key that is a
/// subset of a pool. Children are walked in ascending order, so a lookup is deterministic (Java's
/// `Int2ObjectOpenHashMap` walks them in hash order).
struct IntArrayTrie<V> {
    root: TrieNode<V>,
}

struct TrieNode<V> {
    children: BTreeMap<u32, TrieNode<V>>,
    values: Vec<V>,
}

impl<V> TrieNode<V> {
    fn new() -> TrieNode<V> {
        TrieNode {
            children: BTreeMap::new(),
            values: Vec::new(),
        }
    }
}

impl<V: Copy> IntArrayTrie<V> {
    fn new() -> IntArrayTrie<V> {
        IntArrayTrie {
            root: TrieNode::new(),
        }
    }

    /// `insert`: `key` must be sorted.
    fn insert(&mut self, key: &[u32], value: V) {
        let mut node = &mut self.root;
        for &k in key {
            node = node.children.entry(k).or_insert_with(TrieNode::new);
        }
        node.values.push(value);
    }

    /// `lookup`: the values of every key within `pool`, depth first: a node's own values, then
    /// its children's.
    fn lookup(&self, pool: &BTreeSet<u32>) -> Vec<V> {
        let mut out = Vec::new();
        Self::dfs(&self.root, pool, &mut out);
        out
    }

    fn dfs(node: &TrieNode<V>, pool: &BTreeSet<u32>, out: &mut Vec<V>) {
        out.extend_from_slice(&node.values);
        // Whichever is smaller is walked, as in Java.
        if node.children.len() > pool.len() {
            for key in pool {
                if let Some(child) = node.children.get(key) {
                    Self::dfs(child, pool, out);
                }
            }
        } else {
            for (key, child) in &node.children {
                if pool.contains(key) {
                    Self::dfs(child, pool, out);
                }
            }
        }
    }
}

/// `RecipeTrie`'s `AbstractVariant`: an item or a fluid a basin can hold.
#[derive(Clone, PartialEq, Eq, Hash)]
enum Variant {
    Item(String),
    Fluid(Fluid),
}

/// `RecipeTrie`'s `AbstractIngredient`: the variants an ingredient takes (items sorted), or the
/// universal one for an empty ingredient or a fluid of no amount, which any pool has.
#[derive(PartialEq, Eq, Hash)]
enum Abstract {
    Universal,
    Items(Vec<String>),
    Fluid(Fluid),
}

/// `RecipeTrie` over a slice of recipes: each is stored under the sorted ids of its distinct
/// ingredients (items and fluids), and a lookup finds those whose ingredients the basin's
/// contents can all satisfy.
///
/// Ids go in the order the ingredients are first met, as Java assigns them over the recipe
/// manager's list, where vanilla's recipes come before mods' (data pack load order): the
/// recipes are met by namespace (`minecraft` first), then by id, so the order is the same
/// whichever order the slice has.
struct RecipeTrie {
    trie: IntArrayTrie<usize>,
    /// The ids of the ingredients each variant is one of.
    ingredients: HashMap<Variant, Vec<u32>>,
}

/// `AbstractIngredient.Universal`'s id.
const UNIVERSAL: u32 = 0;

impl RecipeTrie {
    fn new(recipes: &[Recipe]) -> RecipeTrie {
        let mut ids: HashMap<Abstract, u32> = HashMap::from([(Abstract::Universal, UNIVERSAL)]);
        let mut ingredients: HashMap<Variant, Vec<u32>> = HashMap::new();
        let mut trie = IntArrayTrie::new();
        let mut order: Vec<usize> = (0..recipes.len()).collect();
        let met = |i: usize| {
            let id = recipes[i].id.as_str();
            (!id.starts_with("minecraft:"), id)
        };
        order.sort_by(|&a, &b| met(a).cmp(&met(b)));
        for i in order {
            let recipe = &recipes[i];
            let from_items = recipe.ingredients.iter().map(|ingredient| {
                if ingredient.is_empty() {
                    return Abstract::Universal;
                }
                let mut items = ingredient.clone();
                items.sort();
                items.dedup();
                Abstract::Items(items)
            });
            let from_fluids = recipe.fluids.iter().map(|&(fluid, amount)| {
                if amount == 0 {
                    Abstract::Universal
                } else {
                    Abstract::Fluid(fluid)
                }
            });
            let mut key = BTreeSet::new();
            for ingredient in from_items.chain(from_fluids) {
                let id = match ids.get(&ingredient) {
                    Some(&id) => id,
                    None => {
                        let id = ids.len() as u32;
                        let variants: Vec<Variant> = match &ingredient {
                            Abstract::Universal => Vec::new(),
                            Abstract::Items(items) => {
                                items.iter().cloned().map(Variant::Item).collect()
                            }
                            Abstract::Fluid(fluid) => vec![Variant::Fluid(*fluid)],
                        };
                        for variant in variants {
                            ingredients.entry(variant).or_default().push(id);
                        }
                        ids.insert(ingredient, id);
                        id
                    }
                };
                key.insert(id);
            }
            let key: Vec<u32> = key.into_iter().collect();
            trie.insert(&key, i);
        }
        RecipeTrie { trie, ingredients }
    }

    /// `lookup` of `getVariants`: the recipes made of (some of) what the basin holds, in trie
    /// order.
    fn lookup(&self, slots: &[Slot], tanks: &Tanks) -> Vec<usize> {
        let mut pool = BTreeSet::from([UNIVERSAL]);
        let items = slots
            .iter()
            .flatten()
            .map(|(item, _)| Variant::Item(item.clone()));
        let fluids = tanks
            .output
            .iter()
            .chain(&tanks.input)
            .flatten()
            .map(|&(fluid, _)| Variant::Fluid(fluid));
        for variant in items.chain(fluids) {
            if let Some(ids) = self.ingredients.get(&variant) {
                pool.extend(ids);
            }
        }
        self.trie.lookup(&pool)
    }
}

/// `getMatchingRecipes`: the recipes that would apply now, found through `RecipeTrie` and then
/// those with the most ingredients first (a stable sort, so equal counts keep the trie's order).
pub fn matching(recipes: &[Recipe], slots: &[Slot], tanks: &Tanks, cx: &Context) -> Vec<usize> {
    let mut out: Vec<usize> = RecipeTrie::new(recipes)
        .lookup(slots, tanks)
        .into_iter()
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

/// The mixer's cycle (`MechanicalMixerBlockEntity.tick`): 20 ticks down, the processing time at
/// the bottom, 20 ticks up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mixer {
    pub running: bool,
    pub running_ticks: i32,
    pub processing_ticks: i32,
    /// The recipe being mixed (`currentRecipe`), by index.
    pub recipe: Option<usize>,
}

/// What a tick of the mixer asks of the glue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MixStep {
    None,
    /// At the bottom, the time is up: apply the recipe (`applyBasinRecipe`).
    Apply,
    /// The cycle ended: look at the basin again (`basinChecker.scheduleUpdate`).
    Done,
}

/// `Mth.log2`: the floor of the base-2 logarithm.
fn log2(v: i32) -> i32 {
    31 - (v.max(1) as u32).leading_zeros() as i32
}

impl Mixer {
    /// `startProcessingBasin`: leaves `processing_ticks` as it was, so the first cycle of a fresh
    /// mixer (0) spends a tick going to -1 before it works the time out, and a cycle stopped
    /// mid-mix resumes its countdown.
    pub fn start(&mut self, recipe: usize) {
        if self.running && self.running_ticks <= 20 {
            return;
        }
        self.running = true;
        self.running_ticks = 0;
        self.recipe = Some(recipe);
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
        m.start(0);
        let mut applied_at = None;
        for t in 0..200 {
            match m.tick(64.0, 0) {
                MixStep::Apply => applied_at = Some(t),
                MixStep::Done => {
                    // 64 rpm: log2(8) × 15 + 1 = 46 ticks at the bottom, after the tick a fresh
                    // mixer (`processing_ticks` 0) spends going to -1.
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

    /// Ticks `m` at 64 rpm until the cycle ends; the tick the recipe applies on.
    fn run_cycle(m: &mut Mixer) -> i32 {
        let mut applied_at = None;
        for t in 0..200 {
            match m.tick(64.0, 0) {
                MixStep::Apply => applied_at = Some(t),
                MixStep::Done => return applied_at.expect("done before applying"),
                MixStep::None => {}
            }
        }
        panic!("the cycle never ended");
    }

    #[test]
    fn the_first_cycle_of_a_mixer_is_a_tick_longer_than_the_second() {
        let mut m = Mixer::default();
        m.start(0);
        let first = run_cycle(&mut m);
        m.start(0);
        let second = run_cycle(&mut m);
        assert_eq!(first, 20 + 1 + 46);
        assert_eq!(second, 20 + 46);
    }

    #[test]
    fn a_mixer_stopped_mid_mix_resumes_its_countdown() {
        let mut m = Mixer::default();
        m.start(0);
        while m.processing_ticks != 30 {
            assert_eq!(m.tick(64.0, 0), MixStep::None);
        }
        // Stopped: the head goes back up without applying.
        loop {
            match m.tick(0.0, 0) {
                MixStep::Done => break,
                step => assert_eq!(step, MixStep::None),
            }
        }
        assert_eq!(m.processing_ticks, 30);
        m.start(0);
        assert_eq!(m.processing_ticks, 30);
        // Down 20 ticks, then 30 ticks counted down from the kept value.
        assert_eq!(run_cycle(&mut m), 20 + 30 - 1);
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
mod admits_tests {
    use super::*;

    fn same(a: &&str, b: &&str) -> bool {
        a == b
    }

    #[test]
    fn only_the_first_empty_input_takes_an_item() {
        let slots = [Some("a"), None, None];
        assert!(admits_slot(&slots, 1, &"b", same));
        assert!(!admits_slot(&slots, 2, &"b", same));
    }

    #[test]
    fn another_slot_holding_the_item_refuses() {
        let slots = [Some("a"), None];
        assert!(!admits_slot(&slots, 1, &"a", same));
    }

    #[test]
    fn the_slot_holding_the_item_takes_more() {
        let slots = [Some("a"), Some("b")];
        assert!(admits_slot(&slots, 0, &"a", same));
    }

    #[test]
    fn output_slots_refuse() {
        let slots = vec![None; SLOTS];
        assert!(!admits_slot(&slots, OUTPUTS.start, &"a", same));
    }

    #[test]
    fn the_trie_finds_subsets_depth_first_in_key_order() {
        let mut trie = IntArrayTrie::new();
        trie.insert(&[2, 3], "c");
        trie.insert(&[3], "d");
        trie.insert(&[1, 2], "a");
        trie.insert(&[], "e");
        trie.insert(&[1], "b");
        trie.insert(&[1], "b2");
        trie.insert(&[4], "never");
        let lookup = |pool: &[u32]| trie.lookup(&pool.iter().copied().collect());
        assert_eq!(lookup(&[1, 2, 3]), ["e", "b", "b2", "a", "c", "d"]);
        assert_eq!(lookup(&[3, 1]), ["e", "b", "b2", "d"]);
        assert_eq!(lookup(&[2, 3]), ["e", "c", "d"]);
        assert_eq!(lookup(&[2]), ["e"], "a key needs all of its ids");
        assert_eq!(lookup(&[]), ["e"]);
    }

    fn recipe(id: &str, items: &[&str], fluids: &[(Fluid, u32)]) -> Recipe {
        Recipe {
            id: id.into(),
            ingredients: items.iter().map(|i| vec![(*i).into()]).collect(),
            results: vec![Output {
                item: "minecraft:stick".into(),
                count: 1,
                chance: 1.0,
            }],
            time: 0,
            heat: HeatReq::None,
            fluids: fluids.to_vec(),
            fluid_results: vec![],
        }
    }

    #[test]
    fn packed_mud_comes_before_mud_mixing_in_either_list_order() {
        let mud_mixing = recipe(
            "create:mixing/mud_by_mixing",
            &["minecraft:dirt"],
            &[(Fluid::Water, 250)],
        );
        let packed_mud = recipe(
            "minecraft:packed_mud",
            &["minecraft:mud", "minecraft:wheat"],
            &[],
        );
        let mut slots: Vec<Slot> = vec![None; SLOTS];
        slots[0] = Some(("minecraft:dirt".into(), 1));
        slots[1] = Some(("minecraft:mud".into(), 1));
        slots[2] = Some(("minecraft:wheat".into(), 1));
        let mut tanks = Tanks::default();
        tanks.input[0] = Some((Fluid::Water, 1000));
        for list in [
            [mud_mixing.clone(), packed_mud.clone()],
            [packed_mud.clone(), mud_mixing.clone()],
        ] {
            let found: Vec<&str> = matching(&list, &slots, &tanks, &cx(Heat::None))
                .into_iter()
                .map(|i| list[i].id.as_str())
                .collect();
            assert_eq!(
                found,
                ["minecraft:packed_mud", "create:mixing/mud_by_mixing"]
            );
        }
    }

    #[test]
    fn more_ingredients_come_first_whatever_the_trie_order() {
        let two = recipe("minecraft:a", &["minecraft:dirt", "minecraft:mud"], &[]);
        let three = recipe(
            "create:b",
            &["minecraft:dirt", "minecraft:mud", "minecraft:wheat"],
            &[],
        );
        let mut slots: Vec<Slot> = vec![None; SLOTS];
        slots[0] = Some(("minecraft:dirt".into(), 1));
        slots[1] = Some(("minecraft:mud".into(), 1));
        slots[2] = Some(("minecraft:wheat".into(), 1));
        let list = [two, three];
        let found = matching(&list, &slots, &Tanks::default(), &cx(Heat::None));
        assert_eq!(found, vec![1, 0]);
    }
}
