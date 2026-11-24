//! Basins and the machines over them, mirroring glue/processing/basin.rs and spoutput.rs:
//! the heat under a basin, what a landing item, a player or an item handler does to it, and a
//! basin, a mixer and a press each at its turn in the transport walk: the operators look at
//! their basin only when told, at their own turn.
use super::*;
use crate::transport::Stack;
use create_engine::{
    Block,
    basin::{self, MixStep, OperatorAsk},
    belt,
    kinetics::Dir,
    press,
    processing::Rng,
    tank::Fluid,
};

type Slot = create_engine::processing::Slot;

/// A basin's spout: the results waiting to go out (`Overflow`, `FluidOverflow`) and the sides
/// the wrench chose and shut (`PreferredSpoutput`, `DisabledSpoutput`).
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Spout {
    pub overflow: Vec<(String, u8)>,
    pub fluids: Vec<(Fluid, u32)>,
    pub disabled: Vec<Dir>,
    pub preferred: Option<Dir>,
}

/// A basin before and after `basin::apply`, and the item results when it succeeded.
struct Applied {
    before: Vec<Slot>,
    slots: Vec<Slot>,
    fluids_before: basin::Tanks,
    tanks: basin::Tanks,
    made: Option<Vec<(String, u8)>>,
}

/// `Iterate.horizontalDirections`: south, west, north, east.
const HORIZONTAL: [Dir; 4] = [Dir::South, Dir::West, Dir::North, Dir::East];

/// `create:passive_boiler_heaters` as the scenarios place them, with `BlockHelper.isNotUnheated`.
const PASSIVE_HEATERS: [&str; 3] = [
    "minecraft:magma_block",
    "minecraft:campfire",
    "minecraft:soul_campfire",
];

fn below(pos: Pos, n: i32) -> Pos {
    [pos[0], pos[1] - n, pos[2]]
}

fn below_side(pos: Pos, d: Dir) -> Pos {
    let o = d.offset();
    [pos[0] + o[0], pos[1] - 1, pos[2] + o[2]]
}

fn context(heat: create_engine::fan::Heat, spout: bool) -> basin::Context<'static> {
    basin::Context {
        heat,
        max: &crate::basin::limit,
        remainder: &crate::basin::remainder,
        spout,
    }
}

fn dir_name(d: Dir) -> &'static str {
    match d {
        Dir::Up => "up",
        Dir::Down => "down",
        Dir::North => "north",
        Dir::South => "south",
        Dir::West => "west",
        Dir::East => "east",
    }
}

fn dir_of(name: &str) -> Option<Dir> {
    Dir::ALL.into_iter().find(|&d| dir_name(d) == name)
}

/// One of the held stack used up, the container given to the player.
fn spend(
    creative: bool,
    hand: &mut Option<Stack>,
    given: &mut Vec<Stack>,
    leftover: Option<String>,
) {
    if creative {
        return;
    }
    match hand {
        Some(s) if s.count > 1 => s.count -= 1,
        _ => *hand = None,
    }
    given.extend(leftover.map(|item| Stack { item, count: 1 }));
}

impl Run {
    /// `basin_at`.
    pub(super) fn basin_at(&self, p: Pos) -> bool {
        self.w.block_at(0, p) == Some(Block::Basin)
    }

    /// `basin_heat`: `BasinBlockEntity.getHeatLevelOf` of the block under the basin.
    pub(super) fn basin_heat(&self, pos: Pos) -> create_engine::fan::Heat {
        use create_engine::fan::Heat;
        let under = below(pos, 1);
        if let Some((_, info)) = self.w.table.get(self.at(under))
            && let Some(heat) = info.heat
        {
            return heat;
        }
        let (name, props) = parse_state(&self.name(self.at(under)));
        let lit = props
            .iter()
            .find(|(k, _)| k == "lit")
            .is_none_or(|(_, v)| v != "false");
        if PASSIVE_HEATERS.contains(&name.as_str()) && lit {
            Heat::Smouldering
        } else {
            Heat::None
        }
    }

    fn basin_tanks(&self, pos: Pos) -> basin::Tanks {
        self.w.dims[0]
            .basin_fluids
            .get(&pos)
            .copied()
            .unwrap_or_default()
    }

    /// `basin_slots`: the basin's 18 slots, None without a basin there.
    fn basin_slots(&self, pos: Pos) -> Option<Vec<Slot>> {
        if !self.basin_at(pos) {
            return None;
        }
        Some(
            self.basin_slots
                .get(&pos)
                .cloned()
                .unwrap_or_else(|| vec![None; basin::SLOTS]),
        )
    }

    /// `write_basin`: the slots that changed.
    fn write_basin(&mut self, pos: Pos, before: &[Slot], after: &[Slot]) {
        if before == after {
            return;
        }
        self.basin_slots.insert(pos, after.to_vec());
    }

    /// A basin gone: the game spills its container's slots (`Containers.dropContents`).
    pub(super) fn basin_removed(&mut self, pos: Pos) {
        let slots = self.basin_slots.remove(&pos).unwrap_or_default();
        self.spouts.remove(&pos);
        self.spout_busy.remove(&pos);
        for (item, count) in slots.into_iter().flatten() {
            self.drop_item_stack(pos.map(f64::from), &item, count);
        }
    }

    /// `basin_take`: an item landing in (or handed to) the basin goes into its inputs as far
    /// as it fits. The count left.
    pub(super) fn basin_take(&mut self, pos: Pos, stack: &Stack) -> u8 {
        let Some(before) = self.basin_slots(pos) else {
            return stack.count;
        };
        let mut inputs = before[basin::INPUTS].to_vec();
        let left = basin::insert_one_stack(
            &mut inputs,
            &stack.item,
            stack.count,
            crate::basin::limit(&stack.item),
        );
        if left == stack.count {
            return left;
        }
        let mut after = before.clone();
        after[basin::INPUTS].clone_from_slice(&inputs);
        self.write_basin(pos, &before, &after);
        self.w.basin_contents_changed(0, pos);
        left
    }

    /// `processing::belt_input_room` for a basin: `insertItemStacked` over its inputs.
    pub(super) fn basin_room(&self, pos: Pos, stack: &Stack) -> Option<u8> {
        let slots = self.basin_slots(pos)?;
        let mut inputs = slots[basin::INPUTS].to_vec();
        Some(create_engine::processing::insert_stacked(
            &mut inputs,
            &stack.item,
            stack.count,
            crate::basin::limit(&stack.item),
        ))
    }

    /// `BasinBlock.useItemOn` (`use_basin`): what the use left in the hand and gave the player.
    fn basin_use(&mut self, pos: Pos, stack: Option<&Stack>) -> (Option<Stack>, Vec<Stack>) {
        let mut hand = stack.cloned();
        let mut given = Vec::new();
        let Some(s) = stack else {
            let Some(before) = self.basin_slots(pos) else {
                return (None, given);
            };
            let mut slots = before.clone();
            for slot in slots.iter_mut() {
                if let Some((item, count)) = slot.take() {
                    given.push(Stack { item, count });
                }
            }
            let empty = vec![None; basin::SLOTS];
            self.write_basin(pos, &before, &empty);
            self.w.basin_contents_changed(0, pos);
            self.w.basin_emptied(0, pos);
            return (None, given);
        };
        let mut tanks = self.basin_tanks(pos);
        // `tryEmptyItemIntoBE`: all of it into an input tank, or nothing.
        if let Some(e) = crate::fluid_recipes::contents(&s.item) {
            let mut inputs = tanks.input;
            if basin::fill(&mut inputs, e.fluid, e.amount) == e.amount {
                tanks.input = inputs;
                self.w.set_basin_fluids(0, pos, tanks);
                spend(self.creative, &mut hand, &mut given, e.leftover);
                return (hand, given);
            }
        }
        // `tryFillItemFromBE` on `CombinedTankWrapper(output, input)`: the first tank holding
        // what filling the item takes.
        for i in 0..4 {
            let slot = if i < 2 {
                &mut tanks.output[i]
            } else {
                &mut tanks.input[i - 2]
            };
            let Some((fluid, have)) = *slot else {
                continue;
            };
            let Some((need, out)) =
                create_engine::spout::filling(&s.item, fluid, &crate::fluid_recipes::filling())
            else {
                continue;
            };
            if need > have {
                continue;
            }
            *slot = (have > need).then_some((fluid, have - need));
            self.w.set_basin_fluids(0, pos, tanks);
            spend(self.creative, &mut hand, &mut given, Some(out));
            return (hand, given);
        }
        (hand, given)
    }

    /// A player's use on the basin at `p` with `stack` (None: an empty hand): the hand after
    /// and what the player was given. None elsewhere.
    pub(super) fn basin_use_input(
        &mut self,
        p: Pos,
        stack: Option<&Stack>,
    ) -> Option<(Option<Stack>, Vec<Stack>)> {
        self.basin_at(p).then(|| self.basin_use(p, stack))
    }

    /// The basin's item handler (`CombinedInvWrapper` of its two `BasinInventory`s): an insert
    /// into an input slot as `BasinInventory.insertItem` admits it (the game's container asks
    /// the glue's `BasinInventory` part), into an output slot never; an extract gives from any
    /// slot.
    pub(super) fn basin_handler(
        &mut self,
        p: Pos,
        slot: u8,
        insert: Option<&Stack>,
        extract: Option<u8>,
        simulate: bool,
        outcome: &mut crate::transport::Outcome,
    ) -> bool {
        if !self.basin_at(p) {
            return false;
        }
        outcome.handler = Some(true);
        let Some(before) = self.basin_slots(p) else {
            return true;
        };
        let mut slots = before.clone();
        if let Some(s) = insert {
            let left = if basin::INPUTS.contains(&usize::from(slot)) {
                basin::insert_item(
                    &mut slots[basin::INPUTS],
                    usize::from(slot),
                    &s.item,
                    s.count,
                    crate::basin::limit(&s.item),
                )
            } else {
                s.count
            };
            outcome.remainder = (left > 0).then(|| Stack {
                item: s.item.clone(),
                count: left,
            });
        } else if let Some(n) = extract
            && let Some((item, count)) = slots.get(usize::from(slot)).cloned().flatten()
        {
            let k = n.min(count);
            outcome.taken.push(Stack {
                item: item.clone(),
                count: k,
            });
            slots[usize::from(slot)] = (count > k).then_some((item, count - k));
        }
        if simulate || slots == before {
            return true;
        }
        self.basin_slots.insert(p, slots);
        self.w.basin_contents_changed(0, p);
        true
    }

    /// `BasinOperatingBlockEntity.tick`, at the operator's turn and before its own: a basin
    /// emptied or taken away stops it there (`onBasinRemoved`), the base's tick ending early;
    /// else, told of its basin, it looks once (`updateBasin`) and may start on a basin whose
    /// spout is free.
    pub(super) fn operator_tick(&mut self, operator: Pos) -> OperatorAsk {
        let asked = self.w.operator_ask(0, operator);
        if asked == OperatorAsk::Basin {
            let at = below(operator, 2);
            let recipe = self
                .spout_can_continue(at)
                .then(|| self.matching_recipe(operator, at))
                .flatten();
            self.w.operator_start(0, operator, recipe);
        }
        asked
    }

    /// `matchBasinRecipe` as the basin at `at` stands: the recipe applies now and its item
    /// results can go where the basin spouts (`acceptOutputs`, simulated).
    fn recipe_fits(
        &self,
        accepts: bool,
        recipe: &basin::Recipe,
        slots: &[Slot],
        tanks: &basin::Tanks,
        cx: &basin::Context<'_>,
    ) -> bool {
        (recipe.results.is_empty() || accepts)
            && !basin::matching(std::slice::from_ref(recipe), slots, tanks, cx).is_empty()
    }

    /// `matching_recipe`: `getMatchingRecipes` of the operator at `operator`: the fitting
    /// recipe with the most ingredients. A mixer's are its book's; a press's are the compacting
    /// ones, or a square crafting recipe of one item (`canCompress`) that has more ingredients.
    pub(super) fn matching_recipe(&mut self, operator: Pos, at: Pos) -> Option<basin::Recipe> {
        let accepts = self.spout_accepts(at);
        let slots = self.basin_slots(at)?;
        let tanks = self.basin_tanks(at);
        let cx = context(self.basin_heat(at), self.basin_facing(at) != Dir::Down);
        let press = self.w.block_at(0, operator) == Some(Block::MechanicalPress);
        let squares = if press {
            self.compressible(&slots)
        } else {
            Vec::new()
        };
        let fits = |r: &basin::Recipe| self.recipe_fits(accepts, r, &slots, &tanks, &cx);
        if press {
            let compacting = basin::matching(&self.basin_books.compacting, &slots, &tanks, &cx)
                .into_iter()
                .map(|i| self.basin_books.compacting[i].clone())
                .find(|r| fits(r));
            let most = compacting.as_ref().map_or(0, |c| c.ingredients.len());
            squares
                .into_iter()
                .find(|c| c.ingredients.len() > most && fits(c))
                .or(compacting)
        } else {
            basin::matching(&self.basin_books.mixing, &slots, &tanks, &cx)
                .into_iter()
                .map(|i| self.basin_books.mixing[i].clone())
                .find(|r| fits(r))
        }
    }

    /// `applyBasinRecipe`: the operator at `machine`'s recipe on the basin two below it,
    /// applied for real with its results routed; then the basin is told, and a mixer stays down
    /// for the next one when nothing waited at its spout before and the recipe still fits
    /// (`continueWithPreviousRecipe`).
    fn apply_recipe(&mut self, machine: Pos, recipe: &basin::Recipe) {
        let at = below(machine, 2);
        let was_empty = self.spout_can_continue(at);
        let accepts = recipe.results.is_empty() || self.spout_accepts(at);
        let Some(before) = accepts.then(|| self.basin_slots(at)).flatten() else {
            return;
        };
        let mut slots = before.clone();
        let fluids_before = self.basin_tanks(at);
        let mut tanks = fluids_before;
        let mut rng = Rng::new(self.seed_at(machine));
        let cx = context(self.basin_heat(at), self.basin_facing(at) != Dir::Down);
        let made = basin::apply(&mut slots, &mut tanks, recipe, &cx, &mut rng, false);
        let again = made.is_some() && self.recipe_fits(accepts, recipe, &slots, &tanks, &cx);
        self.write_applied(
            at,
            Applied {
                before,
                slots,
                fluids_before,
                tanks,
                made,
            },
        );
        self.w.recipe_applied(0, machine, was_empty && again);
    }

    /// `tick_basin`: `BasinBlockEntity.tick` at the basin's turn in the transport walk: its
    /// lazy tick (the spout, then the backup check), then what changed in it since its last
    /// turn is told to its operator.
    pub(super) fn tick_basin(&mut self, pos: Pos) {
        let lazy = self.w.basin_lazy_tick(0, pos).unwrap_or(false);
        self.tick_spout(pos, lazy);
        if lazy {
            let tanks = self.basin_tanks(pos);
            let empty = self
                .basin_slots(pos)
                .is_none_or(|s| s.iter().all(Option::is_none))
                && tanks.input.iter().chain(&tanks.output).all(Option::is_none);
            self.w.basin_backup_check(0, pos, empty);
        }
        self.w.basin_turn(0, pos);
    }

    /// `MechanicalMixerBlockEntity.tick` at the mixer's turn in the transport walk: its
    /// operator's tick, then the head's cycle, the recipe applied at the bottom.
    pub(super) fn tick_mixer(&mut self, pos: Pos) {
        self.operator_tick(pos);
        if let Some((MixStep::Apply, Some(recipe))) =
            self.w
                .mixer_turn(0, pos, self.w.speed(0, pos).unwrap_or(0.))
        {
            self.apply_recipe(pos, &recipe);
        }
    }

    /// `write_applied`: what a recipe left in its basin.
    fn write_applied(&mut self, pos: Pos, mut a: Applied) {
        if a.made.is_some() {
            self.route_fluids(pos, &a.fluids_before, &mut a.tanks);
        }
        if a.made.is_some() || a.tanks != a.fluids_before {
            self.w.set_basin_fluids(0, pos, a.tanks);
        }
        if let Some(made) = a.made.clone()
            && self.basin_facing(pos) != Dir::Down
        {
            self.route(pos, made);
        }
        self.write_basin(pos, &a.before, &a.slots);
    }

    /// `compressible`: the square crafting recipes (`canCompress`) of the items in the basin,
    /// those with the most ingredients first.
    fn compressible(&mut self, slots: &[Slot]) -> Vec<basin::Recipe> {
        let mut items: Vec<String> = slots.iter().flatten().map(|(i, _)| i.clone()).collect();
        items.sort();
        items.dedup();
        let mut out = Vec::new();
        for item in items {
            let found = match self.compressed.get(&item) {
                Some(found) => found.clone(),
                None => {
                    let found: Vec<basin::Recipe> =
                        self.basin_books.squares(&item).cloned().collect();
                    self.compressed.insert(item, found.clone());
                    found
                }
            };
            out.extend(found);
        }
        out.sort_by_key(|r| std::cmp::Reverse(r.ingredients.len()));
        out
    }

    /// `tryProcessInBasin` at the head's lowest point: the press's recipe (`currentRecipe`) on
    /// the basin.
    fn compact_in_basin(&mut self, pos: Pos) {
        let Some(recipe) = self.current_recipe(pos) else {
            return;
        };
        self.apply_recipe(pos, &recipe);
    }

    /// The operator at `pos`'s `currentRecipe`.
    fn current_recipe(&self, pos: Pos) -> Option<basin::Recipe> {
        self.w.dims[0]
            .basin_operators
            .by_pos
            .get(&pos)
            .and_then(|o| o.current_recipe.clone())
    }

    /// `MechanicalPressBlockEntity.onPressingCompleted` on a basin: its recipe still matches
    /// (`matchBasinRecipe(currentRecipe)`) and the basin can go on (`canContinueProcessing`),
    /// so the next cycle starts now.
    fn restarts_in_basin(&self, pos: Pos) -> bool {
        let at = below(pos, 2);
        let Some(recipe) = self.current_recipe(pos) else {
            return false;
        };
        let Some(slots) = self.basin_slots(at) else {
            return false;
        };
        let tanks = self.basin_tanks(at);
        let cx = context(self.basin_heat(at), self.basin_facing(at) != Dir::Down);
        self.spout_can_continue(at)
            && self.recipe_fits(self.spout_accepts(at), &recipe, &slots, &tanks, &cx)
    }

    /// The press at `at`'s turn in the transport walk: its operator's tick, then
    /// `PressingBehaviour.tick`: its head moves; over a basin it compacts at the bottom and,
    /// its cycle ended, starts the next at once when its recipe still fits.
    pub(super) fn tick_press(&mut self, at: Pos) {
        if self.operator_tick(at) == OperatorAsk::Removed {
            return;
        }
        match self.w.tick_press(0, at) {
            Some((press::Mode::Basin, press::Tick::Down)) => self.compact_in_basin(at),
            Some((press::Mode::Basin, press::Tick::Done)) => {
                let restart = self.restarts_in_basin(at);
                self.w.pressing_completed(0, at, restart);
            }
            _ => {}
        }
    }

    /// `spoutput::facing`: the basin's `FACING`.
    pub(super) fn basin_facing(&self, pos: Pos) -> Dir {
        let (_, props) = parse_state(&self.name(self.at(pos)));
        props
            .iter()
            .find(|(k, _)| k == "facing")
            .and_then(|(_, v)| dir_of(v))
            .filter(|d| *d != Dir::Up)
            .unwrap_or(Dir::Down)
    }

    /// `processing::belt_input_side`: a processing machine taking items from belts.
    pub(super) fn belt_input_side(&self, p: Pos) -> Option<Option<Dir>> {
        let (block, info) = self.w.table.get(self.at(p))?;
        match block {
            Block::MechanicalSaw | Block::Basin | Block::Millstone => Some(None),
            Block::CrushingWheelController => Some(info.facing.filter(|&f| f != Dir::Down)),
            _ => None,
        }
    }

    /// `logistics::direct_input_accepts` of what takes items at `p` for an item moving toward
    /// `side`, the processing machines included.
    fn accepts_from(&self, p: Pos, side: Dir) -> bool {
        match self.belt_input_side(p) {
            Some(Some(only)) => only == side,
            Some(None) => true,
            None => self.direct_input_accepts(p, side),
        }
    }

    /// `BasinBlock.canOutputTo`: a block of air (or a funnel not facing that way) beside the
    /// basin, and below that something taking items from that side.
    fn can_output_to(&self, pos: Pos, d: Dir) -> bool {
        let o = d.offset();
        let neighbour = [pos[0] + o[0], pos[1], pos[2] + o[2]];
        let output = below_side(pos, d);
        if let Some((_, info)) = self.funnel_at(neighbour) {
            if info.facing == Some(d) {
                return false;
            }
        } else if self.host_collision(neighbour) {
            return false;
        }
        (self.direct_input_at(output) || self.belt_input_side(output).is_some())
            && self.accepts_from(output, d)
    }

    /// `acceptOutputs`' `targetInv`: something below the side `d` takes items.
    fn spout_has_target(&self, pos: Pos, d: Dir) -> bool {
        let output = below_side(pos, d);
        if self.belt_location(output).is_some() || self.belt_input_side(output).is_some() {
            return true;
        }
        if let Some((_, info)) = self.funnel_at(output)
            && let Some(facing) = info.facing
        {
            let behind = create_engine::funnel::behind(output, facing);
            return self.item_handler_at(behind);
        }
        self.item_handler_at(output)
    }

    fn item_handler_at(&self, p: Pos) -> bool {
        matches!(
            self.w.block_at(0, p),
            Some(Block::Depot | Block::WeightedEjector)
        )
    }

    /// `handlers::DepotItems.insert` (an ejector's merging one): the count left, None elsewhere.
    fn handler_insert(&mut self, p: Pos, s: &Stack, simulate: bool) -> Option<u8> {
        match self.w.block_at(0, p) {
            Some(Block::Depot) => Some(self.depot_insert(p, s, simulate).map_or(0, |l| l.count)),
            Some(Block::WeightedEjector) => Some(self.ejector_handler_insert(p, s, simulate)),
            _ => None,
        }
    }

    /// `spoutput::insert`: what it can of `count` of `item` into the target below side `d`;
    /// `simulate`: only how much it would.
    fn spout_insert(&mut self, pos: Pos, d: Dir, item: &str, count: u8, simulate: bool) -> u8 {
        let output = below_side(pos, d);
        if let Some(left) = self.put_on_from_above(
            output,
            &Stack {
                item: item.into(),
                count,
            },
        ) {
            return count - left;
        }
        if self.belt_input_side(output).is_some() {
            if simulate {
                return count;
            }
            let one = belt::Item::new(item, count, 0, false);
            let left = self
                .processing_belt_input(output, d, &one, false)
                .unwrap_or(count);
            return count - left;
        }
        let stack = Stack {
            item: item.into(),
            count,
        };
        let at = match self.funnel_at(output) {
            Some((_, info)) => {
                // `filter.test`: a brass funnel's filter.
                if !self.funnel_passes(output, item) {
                    return 0;
                }
                let Some(facing) = info.facing else {
                    return 0;
                };
                create_engine::funnel::behind(output, facing)
            }
            None => output,
        };
        self.handler_insert(at, &stack, simulate)
            .map_or(0, |left| count - left)
    }

    /// `acceptOutputs(.., simulate)` for a recipe with item results: facing a side, something
    /// there must take items.
    fn spout_accepts(&self, pos: Pos) -> bool {
        let d = self.basin_facing(pos);
        d == Dir::Down || self.spout_has_target(pos, d)
    }

    /// `canContinueProcessing`: nothing waits to go out of the spout.
    fn spout_can_continue(&self, pos: Pos) -> bool {
        self.spouts
            .get(&pos)
            .is_none_or(|s| s.overflow.is_empty() && s.fluids.is_empty())
    }

    /// `acceptOutputs` for a recipe's fluid results, after it put them into the basin's
    /// output tanks: facing a side with something holding fluids below it, what it added goes
    /// into the spout's fluid buffer instead.
    fn route_fluids(&mut self, pos: Pos, before: &basin::Tanks, after: &mut basin::Tanks) {
        let d = self.basin_facing(pos);
        if d == Dir::Down || self.fluid_target(pos, d).is_none() {
            return;
        }
        let mut added = Vec::new();
        for i in 0..after.output.len() {
            let Some((f, n)) = after.output[i] else {
                continue;
            };
            let had = before.output[i]
                .filter(|(g, _)| *g == f)
                .map_or(0, |(_, m)| m);
            if n > had {
                added.push((f, n - had));
                after.output[i] = (had > 0).then_some((f, had));
            }
        }
        if added.is_empty() {
            return;
        }
        self.spouts.entry(pos).or_default().fluids.extend(added);
        self.spout_busy.insert(pos);
    }

    /// `acceptOutputs` facing a side: a recipe's item results go into the spout's buffer.
    fn route(&mut self, pos: Pos, results: Vec<(String, u8)>) {
        self.spouts.entry(pos).or_default().overflow.extend(results);
        self.spout_busy.insert(pos);
    }

    /// What holds fluids below the side `d`: an item drain (true) or a basin (false).
    fn fluid_target(&self, pos: Pos, d: Dir) -> Option<bool> {
        let output = below_side(pos, d);
        if self.w.dims[0].drains.contains_key(&output) {
            return Some(true);
        }
        self.basin_at(output).then_some(false)
    }

    /// `forceFill` of `amount` of `fluid` into the target, all of it or none.
    fn fill_whole(&mut self, target: Pos, drain: bool, fluid: Fluid, amount: u32) -> bool {
        if drain {
            let Some(d) = self.w.dims[0].drains.get_mut(&target) else {
                return false;
            };
            let next = match d.fluid {
                None => amount,
                Some((f, n)) if f == fluid => n + amount,
                _ => return false,
            };
            if next > create_engine::drain::CAPACITY {
                return false;
            }
            d.fluid = Some((fluid, next));
            self.w.dims[0].analog_checks.push(target);
            self.w.mark_unsaved(0, target);
            true
        } else {
            let mut tanks = self.basin_tanks(target);
            let mut input = tanks.input;
            if basin::fill(&mut input, fluid, amount) != amount {
                return false;
            }
            tanks.input = input;
            self.w.set_basin_fluids(0, target, tanks);
            true
        }
    }

    /// `BasinBlock.onWrenched`: the spout side is chosen, or shut when it is the one faced.
    pub(super) fn basin_wrenched(&mut self, pos: Pos, face: Dir) {
        let current = self.basin_facing(pos);
        let spout = self.spouts.entry(pos).or_default();
        spout.disabled.retain(|d| *d != face);
        if current == face {
            if spout.preferred == Some(face) {
                spout.preferred = None;
            }
            spout.disabled.push(face);
        } else {
            spout.preferred = Some(face);
        }
        self.spout_update(pos);
    }

    /// `updateSpoutput`: faces the last side it may output to and isn't shut (the chosen one
    /// if it may), else down; turned to a side, what its outputs hold goes out through the spout.
    fn spout_update(&mut self, pos: Pos) {
        let current = self.basin_facing(pos);
        let spout = self.spouts.entry(pos).or_default().clone();
        let mut next = Dir::Down;
        for test in HORIZONTAL {
            if self.can_output_to(pos, test) && !spout.disabled.contains(&test) {
                next = test;
            }
        }
        if let Some(p) = spout.preferred
            && p != Dir::Up
            && self.can_output_to(pos, p)
        {
            next = p;
        }
        if next == current {
            return;
        }
        let state = with_prop(&self.name(self.at(pos)), "facing", dir_name(next));
        self.set(pos, self.id(&state));
        if next == Dir::Down {
            return;
        }
        if self.spout_has_target(pos, next)
            && let Some(before) = self.basin_slots(pos)
        {
            let mut after = before.clone();
            let mut list = spout.overflow;
            for i in basin::OUTPUTS {
                if let Some(s) = after[i].take() {
                    list.push(s);
                }
            }
            if after != before {
                self.write_basin(pos, &before, &after);
                self.spouts.entry(pos).or_default().overflow = list;
                self.spout_busy.insert(pos);
            }
        }
        self.w.basin_contents_changed(0, pos);
    }

    /// The spout's part of `BasinBlockEntity.tick`, at the basin's turn in the transport walk:
    /// on its lazy tick the basin looks where to put its results; what waits at the spout
    /// goes out.
    pub(super) fn tick_spout(&mut self, pos: Pos, lazy: bool) {
        if lazy {
            self.spout_update(pos);
            if !self.spout_can_continue(pos) {
                self.spout_busy.insert(pos);
            }
        }
        if self.spout_busy.contains(&pos) && !self.clear_overflow(pos) {
            self.spout_busy.remove(&pos);
        }
    }

    /// `tryClearingSpoutputOverflow`: each waiting stack into the target as far as it takes
    /// it (facing down they pop out), the fluids whole or not at all. Whether the basin is
    /// still busy.
    fn clear_overflow(&mut self, pos: Pos) -> bool {
        let facing = self.basin_facing(pos);
        let spout = self.spouts.entry(pos).or_default();
        let before_items = spout.overflow.clone();
        let before_fluids = spout.fluids.clone();
        let mut list = std::mem::take(&mut spout.overflow);
        let mut fluids = std::mem::take(&mut spout.fluids);
        let mut i = 0;
        while i < list.len() {
            let (item, n) = list[i].clone();
            if facing == Dir::Down {
                self.pop_resource(pos, &item, n);
                list.remove(i);
                continue;
            }
            if !self.spout_has_target(pos, facing) {
                break;
            }
            let would = self.spout_insert(pos, facing, &item, n, true);
            if would == 0 {
                i += 1;
                continue;
            }
            let took = self.spout_insert(pos, facing, &item, n, false);
            if took >= n {
                list.remove(i);
            } else {
                list[i].1 = n - took;
                i += 1;
            }
        }
        // Nothing holding fluids there: they wait (`targetTank == null`).
        if facing != Dir::Down
            && let Some(drain) = self.fluid_target(pos, facing)
        {
            fluids.retain(|&(f, n)| !self.fill_whole(below_side(pos, facing), drain, f, n));
        }
        let changed = list != before_items || fluids != before_fluids;
        let spout = self.spouts.entry(pos).or_default();
        spout.overflow = list;
        spout.fluids = fluids;
        // `notifyChangeOfContents`: its operator is told.
        if changed {
            self.w.basin_contents_changed(0, pos);
        }
        !spout.overflow.is_empty() || !spout.fluids.is_empty()
    }

    /// `processing::belt_input` for the basin at `pos`: `basin_take` of the stack. The count left.
    pub(super) fn basin_belt_input(&mut self, pos: Pos, item: &belt::Item) -> u8 {
        self.basin_take(
            pos,
            &Stack {
                item: item.item.clone(),
                count: item.count,
            },
        )
    }
}
