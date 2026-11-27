//! The deployer (`DeployerBlockEntity`, `DeployerHandler`): its stroke, its recipes over a
//! depot or a belt (`BeltDeployerCallbacks`), and its fake player (`DeployerFakePlayer`) using
//! what it holds on the block it reaches, or punching what is there.
//!
//! The fake player is the game's (`ServerHost::fake_player`), acting with the deployer's held
//! stack for one action at a time (`perform_action_scoped`): an entity in the block it reaches is
//! used first (`interactOn`: shears on a sheep, a bucket on a cow, a name tag), then its click is
//! the game's own path (`ServerPlayerGameMode.useItemOn`: the block's use, then the item's
//! `useOn`, which places a block item, tills with a hoe...), then the item's use in the air
//! (`item.use`: a bucket fills or empties, a bottle fills, a snowball is thrown); its punch the
//! game's `Player.attack` (`PlayerRef::attack`, run outside `perform_action_scoped`, which
//! refuses it) and block break. Sand paper polishes what lies under it. A player in front whose
//! interaction passed is fed what it holds, if it is a food they can eat or a deployable
//! drink (`finishUsingItem` on them, `finish_using`). A punch digs at the pace the
//! fake player holding the whole stack gets from the game (`PlayerRef::destroy_progress`, Create's
//! `getDestroyProgress`: tool, enchantments, and the slower digging of a block whose drops need
//! another tool).

use super::takes_from_side;
use super::{press, recipes, stacks};
use crate::glue::names::Named as _;
use crate::glue::net;
use crate::glue::owned::LevelLists as _;
use crate::glue::server::ServerLevel;
use crate::glue::world::with_world;
use create_engine::Block;
use create_engine::deployer::{self, Deployer};
use create_engine::kinetics::Dir;
use create_engine::{belt, processing};
use mod_api::{
    BlockClick, BlockHit, Hand, HostBlockStates as _, ItemStack, PlayerAction, PlayerActionSetup,
    PlayerRef, ServerHost,
};

/// `DeployerGameProfile`'s fallback name.
const FAKE_NAME: &str = "Deployer";

/// The deployer `d`'s fake player (`DeployerFakePlayer`): its owner's UUID, the owner's name
/// while they are online (Create: the last one known), else Create's fallback profile.
fn fake_player(lv: &mut ServerLevel<'_>, d: &Deployer) -> Option<PlayerRef> {
    let uuid = d.fake_uuid();
    let name = d
        .owner
        .and_then(|o| lv.host.player(o))
        .and_then(|p| p.name(&*lv.host).map(str::to_owned))
        .unwrap_or_else(|| FAKE_NAME.to_owned());
    lv.host.fake_player(lv.level, uuid, &name)
}

/// `minecraft:item`, for the crumbs of what a deployer feeds.
pub(crate) static ITEM_PARTICLE: std::sync::OnceLock<mod_api::ParticleKey> =
    std::sync::OnceLock::new();

/// The deployer two above `depot`, facing down, and its speed; None for anything else.
pub fn deployer_above(lv: &ServerLevel<'_>, depot: [i32; 3]) -> Option<(Deployer, f32)> {
    let above = [depot[0], depot[1] + 2, depot[2]];
    if lv.facing_of(above, Block::Deployer) != Some(Dir::Down) {
        return None;
    }
    let dim = lv.dim;
    let d = with_world(|w| w.dims[dim].deployers.get(&above).cloned()).flatten()?;
    Some((d, lv.speed(above)))
}

/// The deployer's recipe for `item` with what it holds (`DeployerBlockEntity.getRecipe`): sand
/// paper polishes first (`SANDPAPER_POLISHING`), then a sequenced assembly's deploying step,
/// else a deploying or item application recipe.
fn recipe_for(
    item: &str,
    progress: Option<&create_engine::sequenced::Progress>,
    tool: &str,
) -> (
    Option<create_engine::sequenced::Assembly>,
    Option<deployer::Recipe>,
) {
    if matches!(tool, "create:sand_paper" | "create:red_sand_paper") {
        let polishing = recipes::with(|r| {
            let id = r.polishing.find_single(item)?;
            let p = r.polishing.get(id);
            Some(deployer::Recipe {
                id: p.id.clone(),
                target: p.ingredients.first().cloned().unwrap_or_default(),
                tool: vec![tool.to_owned()],
                results: p.results.clone(),
                keep_held: false,
            })
        })
        .flatten();
        if polishing.is_some() {
            return (None, polishing);
        }
    }
    let sequenced = press::sequenced_for(item, progress, "create:deploying", Some(tool));
    let recipe = match &sequenced {
        Some((a, s)) => Some(deployer::Recipe {
            id: a.id.clone(),
            target: Vec::new(),
            tool: s.with.clone().unwrap_or_default(),
            results: Vec::new(),
            keep_held: s.keep_held,
        }),
        None => recipes::with(|r| deployer::find(&r.deploying, item, tool).cloned()).flatten(),
    };
    (sequenced.map(|(a, _)| a), recipe)
}

/// `RecipeApplier.applyRecipeOn` on one of the stack, `getMaxOutputCount` 4.
fn rolled(lv: &ServerLevel<'_>, at: [i32; 3], r: &deployer::Recipe) -> Vec<(String, u8)> {
    let seed = lv.game_time() as u64 ^ (at[0] as u64).wrapping_mul(0x9E37_79B9);
    let p = processing::Recipe {
        id: r.id.clone(),
        ingredients: Vec::new(),
        results: r.results.clone(),
        time: 0,
    };
    processing::roll(&p, &mut processing::Rng::new(seed))
        .into_iter()
        .take(4)
        .collect()
}

/// How far `start`'s ray ([`deployer::start_ray`]) from the deployer at `pos` facing `facing`
/// runs before a block's outline stops it (`level.clip`, no fluids): 1 when nothing does.
fn start_ray_hit(lv: &ServerLevel<'_>, pos: [i32; 3], facing: Dir) -> f64 {
    let (from, to) = deployer::start_ray(pos, facing);
    let at = lv
        .level
        .clip(
            &*lv.host,
            from,
            to,
            mod_api::ClipBlock::Outline,
            mod_api::ClipFluid::None,
        )
        .map_or(to, |hit| hit.location);
    (0..3)
        .map(|i| (at[i] - from[i]).powi(2))
        .sum::<f64>()
        .sqrt()
}

/// The deployer's stroke starting toward a depot or belt (`start`).
fn start_over(lv: &ServerLevel<'_>, d: &mut Deployer, at: [i32; 3]) {
    let above = [at[0], at[1] + 2, at[2]];
    d.start(start_ray_hit(lv, above, Dir::Down));
    let dim = lv.dim;
    with_world(|w| {
        let dd = &mut w.dims[dim];
        dd.deployers.insert(above, d.clone());
        dd.deployer_changes.push(above);
    });
}

/// `BeltDeployerCallbacks` for the depot at `depot`, at its turn with its stack arrived.
pub fn deployer_on_depot(lv: &mut ServerLevel<'_>, depot: [i32; 3]) {
    use deployer::Depot;
    let dim = lv.dim;
    let Some((mut d, rpm)) = deployer_above(lv, depot) else {
        return;
    };
    let above = [depot[0], depot[1] + 2, depot[2]];
    let Some(mut held) =
        with_world(|w| w.dims[dim].depots.get(&depot).copied().flatten()).flatten()
    else {
        return;
    };
    if held.sliding() && held.position < 0.5 - 1.0 / 16.0 {
        return;
    }
    let Some(stack) = lv.slot(depot, 0) else {
        return;
    };
    let item = stacks::id(&*lv.host, &stack);
    let count = stack.count().min(255) as u8;
    if !recipes::ready() {
        return;
    }
    let tool = d.held.as_ref().map(|h| h.0.clone()).unwrap_or_default();
    let sequenced = press::sequenced_on_depot(lv, depot, "create:deploying", Some(&tool));
    let (_, recipe) = recipe_for(&item, stacks::sequenced_progress(&stack).as_ref(), &tool);
    match deployer::on_depot(&d, rpm, held.locked, recipe.is_some()) {
        Depot::Pass => held.locked = false,
        Depot::Hold => held.locked = true,
        Depot::Start => {
            start_over(lv, &mut d, depot);
            held.locked = true;
        }
        Depot::Apply => {
            let Some(r) = recipe else {
                return;
            };
            held = match &sequenced {
                Some((a, progress, _)) => {
                    press::sequenced_step_on_depot(lv, depot, count, held, a, progress.as_ref())
                }
                None => {
                    let results = rolled(lv, depot, &r);
                    press::results_on_depot(lv, depot, count, held, results)
                }
            };
            held.locked = true;
            deployer_used(lv, &mut d, &r, above);
            with_world(|w| w.set_deployer(dim, above, d.clone()));
        }
    }
    with_world(|w| {
        if let Some(h) = w.dims[dim].depots.get_mut(&depot) {
            *h = h.map(|_| held);
        }
    });
}

/// `BeltDeployerCallbacks` for a stack on the belt at `at` under the deployer `d` (facing down,
/// at `rpm`): the deployer starts its stroke on a stack its held item applies to (a sequenced
/// assembly's deploying step first) and, back at the top of it, turns one of the stack into the
/// results (`activate`).
pub fn deployer_on_belt(
    lv: &mut ServerLevel<'_>,
    at: [i32; 3],
    item: &mut belt::Item,
    locked: bool,
    mut d: Deployer,
    rpm: f32,
) -> belt::Process {
    use belt::Process;
    use deployer::Depot;
    let dim = lv.dim;
    let above = [at[0], at[1] + 2, at[2]];
    if !recipes::ready() {
        return Process::Pass;
    }
    let tool = d.held.as_ref().map(|h| h.0.clone()).unwrap_or_default();
    let (sequenced, recipe) = recipe_for(
        create_engine::arm::item_id(&item.item),
        item.progress.as_ref(),
        &tool,
    );
    match deployer::on_depot(&d, rpm, locked, recipe.is_some()) {
        Depot::Pass => Process::Pass,
        Depot::Hold => Process::Hold,
        Depot::Start => {
            start_over(lv, &mut d, at);
            Process::Hold
        }
        Depot::Apply => {
            let Some(r) = recipe else {
                return Process::Pass;
            };
            let outputs = match &sequenced {
                Some(a) => press::sequenced_on_belt(lv, at, item, a),
                None => rolled(lv, at, &r)
                    .into_iter()
                    .map(|(name, n)| {
                        let mut out = item.processing_copy();
                        out.item = name;
                        out.count = n;
                        out.progress = None;
                        out
                    })
                    .collect(),
            };
            let outputs = press::on_belt(lv, at, outputs);
            deployer_used(lv, &mut d, &r, above);
            with_world(|w| w.set_deployer(dim, above, d.clone()));
            Process::Convert {
                held: Some(item.left_of_one()),
                outputs,
            }
        }
    }
}

/// The deployer at `pos` after applying `recipe` (`activate`): its tool wears, other items are
/// used up (`Deployer::use_for`); a crafting remainder that finds no room in the hand goes into
/// the fake player's inventory (`getInventory().add`).
fn deployer_used(
    lv: &mut ServerLevel<'_>,
    d: &mut Deployer,
    recipe: &deployer::Recipe,
    pos: [i32; 3],
) {
    crate::glue::advancements::deployer_applied(lv, pos, recipe.results.first().map(|o| &*o.item));
    let held = held_of(lv, d);
    let host = &*lv.host;
    let max_damage = held
        .as_ref()
        .and_then(|s| host.max_damage(s.item()))
        .unwrap_or(0);
    let remainder = held
        .as_ref()
        .and_then(|s| host.item_crafting_remainder(s))
        .map(|s| stacks::id(host, &s));
    if let Some(rest) = d.use_for(recipe, max_damage, remainder.as_deref())
        && let Some(s) = stacks::stack(&*lv.host, &rest, 1)
    {
        place_back(lv, pos, d, vec![s]);
    }
    // `BeltDeployerCallbacks.activate`: the held item breaking, an item picked up, and sand
    // paper's short sanding.
    if d.held.is_none() {
        lv.play_block_sound("minecraft:entity.item.break", pos, 0.25, 1.0);
    }
    lv.play_block_sound("minecraft:entity.item.pickup", pos, 0.25, 0.75);
    if recipe
        .tool
        .iter()
        .any(|t| matches!(t.as_str(), "create:sand_paper" | "create:red_sand_paper"))
    {
        lv.play_block_sound("create:sanding_short", pos, 0.35, 1.0);
    }
}

fn centre(pos: [i32; 3]) -> [f64; 3] {
    pos.map(|v| f64::from(v) + 0.5)
}

/// A deployer's stack rebuilt from its id, count, saved components and wear: None when `make`
/// refuses the id; `Ok` when the saved components (empty, or a patch that decodes to a compound
/// and that `set_components` accepts) were restored; `Err` with the stack as it stands, its
/// components missing, when a non-empty patch could not be.
fn rebuild(
    st: &deployer::Stack,
    make: impl FnOnce(&str, u32) -> Option<ItemStack>,
) -> Option<Result<ItemStack, ItemStack>> {
    let mut s = make(&st.item, u32::from(st.count))?;
    let restored = st.components.is_empty()
        || if let Ok((mod_api::Tag::Compound(c), _)) =
            mod_api::Tag::from_network_bytes(&st.components, None)
        {
            s.set_components(c).is_ok()
        } else {
            false
        };
    if st.damage > 0 {
        s.set_damage(st.damage);
    }
    Some(if restored { Ok(s) } else { Err(s) })
}

/// A deployer's stack as an item stack, with its components and its wear; components that
/// cannot be restored are dropped (the stack is handed back to the world or a player, and a
/// refusal would lose it).
pub(crate) fn item_stack(
    st: &deployer::Stack,
    make: impl FnOnce(&str, u32) -> Option<ItemStack>,
) -> Option<ItemStack> {
    match rebuild(st, make)? {
        Ok(s) | Err(s) => Some(s),
    }
}

/// The deployer's held stack as an item stack, with its components and its wear.
pub(crate) fn held_stack(
    d: &Deployer,
    make: impl FnOnce(&str, u32) -> Option<ItemStack>,
) -> Option<ItemStack> {
    item_stack(&d.held_stack()?, make)
}

/// The deployer's held stack as an item stack, with its components and its wear.
fn held_of(lv: &ServerLevel<'_>, d: &Deployer) -> Option<ItemStack> {
    held_stack(d, |id, n| stacks::stack(&*lv.host, id, n))
}

/// `stack` as a deployer keeps it: its id, count, wear and the rest of its components.
fn engine_stack(host: &dyn mod_api::Host, stack: &ItemStack) -> deployer::Stack {
    deployer::Stack {
        item: stacks::id(host, stack),
        count: stack.count().min(64) as u8,
        damage: stack.damage(),
        components: stack
            .components()
            .and_then(|c| {
                mod_api::Tag::Compound(c.as_nbt().clone())
                    .to_network_bytes()
                    .ok()
            })
            .unwrap_or_default(),
    }
}

/// The deployer's hand takes `stack` (its id, count, wear and the rest of its components).
pub(crate) fn set_held_stack(
    host: &dyn mod_api::Host,
    d: &mut Deployer,
    stack: Option<&ItemStack>,
) {
    d.set_held(stack.map(|s| engine_stack(host, s)));
}

/// `Inventory.placeItemBackInInventory` on the deployer `d`'s fake player for each of `stacks`;
/// what finds no room drops at the deployer.
fn place_back(
    lv: &mut ServerLevel<'_>,
    pos: [i32; 3],
    d: &mut Deployer,
    stacks_in: Vec<ItemStack>,
) {
    for s in stacks_in {
        let max = lv.host.max_stack_size(s.item()).unwrap_or(64).min(64) as u8;
        let st = engine_stack(&*lv.host, &s);
        if let Some(left) = d.place_back(st, max)
            && let Some(left) = item_stack(&left, |id, n| stacks::stack(&*lv.host, id, n))
        {
            lv.spawn_stack(centre(pos), left, [0.0; 3]);
        }
    }
}

/// `DeployerItemHandler`, the deployer's `ITEMS` handler on every side: its overflow's slots
/// (`overflowItems`), then the hand. Out of either; into the hand only, what its filter lets
/// through (`isItemValid`), and not out of it while a filter it passes is set. The hand is the
/// engine's, so an insert or take the host's transaction later aborts cannot be taken back (as
/// with a crushing wheel controller's handler); the overflow is the block entity's slots.
pub struct DeployerItems {
    pub level: mod_api::LevelRef,
    pub dim: usize,
    pub pos: [i32; 3],
}

impl DeployerItems {
    fn entity(&self, host: &dyn mod_api::Host) -> Option<mod_api::BlockEntityRef> {
        host.block_entity(self.level, self.pos)
    }

    fn overflow(&self, host: &dyn mod_api::Host) -> Vec<ItemStack> {
        let Some(e) = self.entity(host) else {
            return Vec::new();
        };
        (0..OVERFLOW_SLOTS as u32)
            .filter_map(|i| e.item_slot(host, i).ok().flatten())
            .collect()
    }

    fn deployer(&self) -> Option<Deployer> {
        with_world(|w| w.dims[self.dim].deployers.get(&self.pos).cloned()).flatten()
    }

    fn set_deployer(&self, d: Deployer) {
        with_world(|w| w.set_deployer(self.dim, self.pos, d));
    }

    /// `filtering.test`: the filter (none lets all through) on `item`.
    fn passes(&self, host: &dyn mod_api::Host, item: &str) -> Option<bool> {
        let f = super::filtering::filter_at(self.dim, self.pos)?;
        Some(crate::glue::items::filters::passes_item(
            host,
            Some(&f),
            item,
        ))
    }

    fn slot(&self, host: &dyn mod_api::Host, index: usize) -> Option<ItemStack> {
        let overflow = self.overflow(host);
        if index < overflow.len() {
            return overflow.into_iter().nth(index);
        }
        held_stack(&self.deployer()?, stacks::known_stack)
    }
}

impl mod_api::ResourceHandler<mod_api::ItemResource> for DeployerItems {
    fn size(&self, host: &mut dyn ServerHost) -> usize {
        1 + self.overflow(&*host).len()
    }
    fn resource(&self, host: &mut dyn ServerHost, index: usize) -> Option<mod_api::ItemResource> {
        self.slot(&*host, index)
            .map(|s| mod_api::ItemResource::of(&s))
    }
    fn amount(&self, host: &mut dyn ServerHost, index: usize) -> u32 {
        self.slot(&*host, index).map_or(0, |s| s.count())
    }
    /// `getSlotLimit`: the slot's stack's size, else 64.
    fn capacity(
        &self,
        host: &mut dyn ServerHost,
        index: usize,
        resource: Option<&mod_api::ItemResource>,
    ) -> u32 {
        let stack = resource
            .map(|r| r.as_stack().clone())
            .or_else(|| self.slot(&*host, index));
        stack.map_or(64, |s| host.max_stack_size(s.item()).unwrap_or(64))
    }
    fn is_valid(
        &self,
        host: &mut dyn ServerHost,
        _index: usize,
        resource: &mod_api::ItemResource,
    ) -> bool {
        let item = stacks::id(&*host, resource.as_stack());
        self.passes(&*host, &item) != Some(false)
    }
    fn insert_at(
        &self,
        host: &mut dyn ServerHost,
        _tx: &mod_api::TransactionContext,
        index: usize,
        resource: &mod_api::ItemResource,
        amount: u32,
    ) -> Result<u32, mod_api::TransactionError<mod_api::Overanswered>> {
        if index != self.overflow(&*host).len() {
            return Ok(0);
        }
        let Some(mut d) = self.deployer() else {
            return Ok(0);
        };
        let Some(stack) = resource.to_stack(amount.min(64)) else {
            return Ok(0);
        };
        let st = engine_stack(&*host, &stack);
        let max = host.max_stack_size(stack.item()).unwrap_or(64).min(64) as u8;
        let valid = self.passes(&*host, &st.item) != Some(false);
        let n = d.insert_held(&st, max, valid, false);
        if n > 0 {
            self.set_deployer(d);
        }
        Ok(u32::from(n))
    }
    fn extract_at(
        &self,
        host: &mut dyn ServerHost,
        tx: &mod_api::TransactionContext,
        index: usize,
        resource: &mod_api::ItemResource,
        amount: u32,
    ) -> Result<u32, mod_api::TransactionError<mod_api::Overanswered>> {
        let overflow = self.overflow(&*host);
        if index < overflow.len() {
            let have = &overflow[index];
            if !resource.matches(have) {
                return Ok(0);
            }
            let n = amount.min(have.count());
            let Some(e) = self.entity(&*host) else {
                return Ok(0);
            };
            // `overflowItems.remove(slot)` when it is emptied: the ones after move up.
            let mut list = overflow.clone();
            match list[index].clone().shrink(n) {
                Some(left) => list[index] = left,
                None => {
                    list.remove(index);
                }
            }
            for i in index..overflow.len() {
                let write = e.set_item_slot(host, tx, i as u32, list.get(i).cloned());
                if let Err(mod_api::BlockEntityItemError::InvalidContext) = write {
                    return Err(mod_api::TransactionError::InvalidContext);
                }
            }
            return Ok(n);
        }
        let Some(mut d) = self.deployer() else {
            return Ok(0);
        };
        let Some(held) = held_stack(&d, stacks::known_stack) else {
            return Ok(0);
        };
        if !resource.matches(&held) {
            return Ok(0);
        }
        let keeps = self.passes(&*host, &stacks::id(&*host, &held)) == Some(true);
        let taken = d.extract_held(amount.min(64) as u8, keeps, false);
        let n = taken.map_or(0, |t| u32::from(t.count));
        if n > 0 {
            self.set_deployer(d);
        }
        Ok(n)
    }
}

/// `DeployerBlock.setPlacedBy`: a deployer a player placed (`entity_place`'s placer, a fake one
/// too) is theirs, once its block is written.
pub(crate) fn placed_by(dim: usize, pos: [i32; 3], player: u128) {
    PLACED.with_borrow_mut(|p| p.push((dim, pos, player)));
}

/// The block at `pos` of level `dim` was written: a deployer [`placed_by`] a player takes them as
/// its owner.
pub(crate) fn placed(dim: usize, pos: [i32; 3]) {
    let Some(owner) = PLACED.with_borrow_mut(|p| {
        let i = p.iter().position(|e| e.0 == dim && e.1 == pos)?;
        Some(p.swap_remove(i).2)
    }) else {
        return;
    };
    with_world(|w| {
        if let Some(d) = w.dims[dim].deployers.get_mut(&pos) {
            d.owner = Some(owner);
            w.mark_unsaved(dim, pos);
        }
    });
}

thread_local! {
    /// Deployers a player placed, until their block is written: level, position, player.
    static PLACED: std::cell::RefCell<Vec<(usize, [i32; 3], u128)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// The world closed.
pub(crate) fn clear() {
    PLACED.with_borrow_mut(Vec::clear);
}

/// `DeployerBlock.neighborChanged` (`redstoneUpdate`): a deployer whose neighbours changed is
/// locked while powered.
pub fn redstone_updates(lv: &mut ServerLevel<'_>) {
    let dim = lv.dim;
    let power =
        with_world(|w| std::mem::take(&mut w.dims[dim].deployer_power_checks)).unwrap_or_default();
    for pos in power {
        if lv.facing_of(pos, Block::Deployer).is_none() {
            continue;
        }
        let powered = lv.has_neighbor_signal(pos);
        with_world(|w| {
            if let Some(d) = w.dims[dim].deployers.get_mut(&pos)
                && d.locked != powered
            {
                d.locked = powered;
                w.mark_unsaved(dim, pos);
            }
        });
    }
}

/// The world around the deployer at `pos` for its turn ([`Deployer::turn`]).
struct Turn<'a, 'b> {
    lv: &'a mut ServerLevel<'b>,
    pos: [i32; 3],
    facing: Dir,
}

impl deployer::World for Turn<'_, '_> {
    fn lost_block(&mut self, at: [i32; 3]) -> bool {
        if !self.lv.is_air(at) {
            return false;
        }
        let breaker = deployer::breaker_id(self.pos);
        let _ = self
            .lv
            .level
            .set_break_progress(&mut *self.lv.host, breaker, at, -1);
        true
    }

    fn overflow(&mut self) -> usize {
        overflow(self.lv, self.pos).len()
    }

    fn held_passes(&mut self, d: &Deployer) -> bool {
        let lv = &*self.lv;
        super::filtering::filter_at(lv.dim, self.pos).is_none_or(|f| {
            d.held.as_ref().is_none_or(|held| {
                crate::glue::items::filters::passes_item(&*lv.host, Some(&f), &held.0)
            })
        })
    }

    fn add_overflow(&mut self, stacks_in: Vec<deployer::Stack>) {
        let host = &*self.lv.host;
        let stacks_in = stacks_in
            .iter()
            .filter_map(|s| item_stack(s, |id, n| stacks::stack(host, id, n)))
            .collect();
        add_overflow(self.lv, self.pos, stacks_in);
    }

    fn should_activate(&mut self, d: &Deployer) -> bool {
        let (lv, facing) = (&*self.lv, self.facing);
        let reached = super::step(self.pos, facing, 2);
        let host = &*lv.host;
        let key = d.held.as_ref().and_then(|h| stacks::key(host, &h.0));
        let places_there = key
            .and_then(|k| host.item_block(k))
            .is_some_and(|b| lv.level.block(host, reached) == Some(b));
        let bucket_there = key
            .and_then(|k| k.bucket_fluid(&host.block_states()))
            .is_some_and(|f| {
                lv.level
                    .fluid_state(host, reached)
                    .is_some_and(|s| s.fluid == f)
            });
        deployer::should_activate(
            d.held.is_some(),
            places_there,
            bucket_there,
            facing == Dir::Down && takes_from_side(lv.dim, reached),
        )
    }

    fn start_fist_bump(&mut self, d: &mut Deployer) -> bool {
        super::fist::start(self.lv, self.pos, self.facing, d)
    }

    fn ray_hit(&mut self) -> f64 {
        start_ray_hit(self.lv, self.pos, self.facing)
    }

    fn trigger_fist_bump(&mut self, d: &mut Deployer) {
        super::fist::trigger(self.lv, self.pos, self.facing, d);
    }

    fn activate(&mut self, d: &mut Deployer) {
        deployer_activate(self.lv, self.pos, self.facing, d);
    }
}

/// `DeployerBlockEntity.tick` at the deployer's turn in the level's block-entity order:
/// its stroke ([`Deployer::turn`]), after the walk's `KineticBlockEntity.tick`.
pub(crate) fn deployer_turn(lv: &mut ServerLevel<'_>, pos: [i32; 3]) {
    let dim = lv.dim;
    let Some(facing) = lv.facing_of(pos, Block::Deployer) else {
        return;
    };
    let Some(mut d) = with_world(|w| w.dims[dim].deployers.get(&pos).cloned()).flatten() else {
        return;
    };
    let rpm = lv.speed(pos);
    let changed = d.turn(rpm, &mut Turn { lv, pos, facing });
    with_world(|w| {
        let dd = &mut w.dims[dim];
        if !dd.deployers.contains_key(&pos) {
            return;
        }
        dd.deployers.insert(pos, d);
        if changed {
            dd.deployer_changes.push(pos);
            dd.unsaved.push((pos[0] >> 4, pos[2] >> 4));
        }
    });
}

/// After the walk: the cracks of removed deployers go, and clients see the strokes that changed.
pub fn send_deployer_changes(lv: &mut ServerLevel<'_>) {
    let dim = lv.dim;
    let Some((changes, cracks)) = with_world(|w| {
        let d = &mut w.dims[dim];
        let mut c = std::mem::take(&mut d.deployer_changes);
        c.sort();
        c.dedup();
        (c, std::mem::take(&mut d.deployer_cracks))
    }) else {
        return;
    };
    // `DeployerFakePlayer.remove`: a removed deployer's cracks go.
    for (pos, at) in cracks {
        let _ = lv
            .level
            .set_break_progress(&mut *lv.host, deployer::breaker_id(pos), at, -1);
    }
    if !changes.is_empty() {
        let players = lv.players();
        send_deployers(lv, &players, &changes);
    }
}

/// `DeployerBlockEntity.activate`: nothing over a depot or belt (their callbacks do the work);
/// punching, `punch`; else the fake player clicks the block it reaches with what it holds
/// (`activateInner` in `Mode.USE`), and the hand keeps what is left of the stack.
fn deployer_activate(lv: &mut ServerLevel<'_>, pos: [i32; 3], facing: Dir, d: &mut Deployer) {
    let reached = deployer::clicked(pos, facing);
    if facing == Dir::Down && takes_from_side(lv.dim, reached) {
        return;
    }
    d.activated = true;
    let other = activate(lv, pos, facing, d, deployer::breaker_id(pos));
    crate::glue::advancements::award_placer(lv, pos, "deployer");
    place_back(lv, pos, d, other);
}

/// The slots a deployer keeps `overflowItems` in, for funnels and hoppers to take; it waits while
/// they hold any. Create's list never grows past eleven (`tick` stops adding at more than ten).
pub const OVERFLOW_SLOTS: usize = 11;

/// `overflowItems` of the deployer at `pos`, in order.
fn overflow(lv: &ServerLevel<'_>, pos: [i32; 3]) -> Vec<ItemStack> {
    lv.slots(pos, OVERFLOW_SLOTS)
        .unwrap_or_default()
        .into_iter()
        .flatten()
        .collect()
}

/// `overflowItems.add` for each of `stacks`: after what it holds, none merged; what finds no slot
/// drops at the deployer.
fn add_overflow(lv: &mut ServerLevel<'_>, pos: [i32; 3], stacks_in: Vec<ItemStack>) {
    if stacks_in.is_empty() {
        return;
    }
    let Some(before) = lv.slots(pos, OVERFLOW_SLOTS) else {
        for s in stacks_in {
            lv.spawn_stack(centre(pos), s, [0.0; 3]);
        }
        return;
    };
    let mut list: Vec<ItemStack> = before.iter().flatten().cloned().collect();
    for s in stacks_in {
        if list.len() < OVERFLOW_SLOTS {
            list.push(s);
        } else {
            lv.spawn_stack(centre(pos), s, [0.0; 3]);
        }
    }
    let changes: Vec<(usize, Option<ItemStack>)> = (0..OVERFLOW_SLOTS)
        .map(|i| (i, list.get(i).cloned()))
        .filter(|(i, s)| before[*i] != *s)
        .collect();
    if !changes.is_empty() {
        lv.set_slots(pos, &changes);
    }
}

/// `DeployerHandler.activate` for the deployer `d` at `pos` facing `facing` (one standing in the
/// world, or riding a contraption there now): punching, `punch`; else its fake player clicks
/// the block two ahead with what it holds (`activateInner` in `Mode.USE`), and the hand keeps
/// what is left of the stack. `breaker` names its cracks. What else the fake ended with
/// (Create's `overflowItems`).
pub fn activate(
    lv: &mut ServerLevel<'_>,
    pos: [i32; 3],
    facing: Dir,
    d: &mut Deployer,
    breaker: i32,
) -> Vec<ItemStack> {
    if d.mode == deployer::Mode::Punch {
        return punch(lv, pos, facing, d, breaker);
    }
    // `safeOnUse` and the item's use, clicked at the middle of the face toward the deployer.
    let reached = deployer::clicked(pos, facing);
    let o = facing.offset();
    let hit = BlockHit::new(
        std::array::from_fn(|i| f64::from(reached[i]) + 0.5 - f64::from(o[i]) * 0.5),
        deployer::clicked_face(facing).offset(),
        false,
    );
    // An entity in the block it reaches is used first (`Player.interactOn`), one at random but
    // a contraption's.
    let targets = entities_at(lv, reached);
    if !targets.is_empty() {
        let target = targets[random_index(lv, targets.len())];
        let location = [0.0, 0.5, 0.0];
        let (outcome, other) = act_with(
            lv,
            pos,
            facing,
            d,
            PlayerAction::InteractOn {
                hand: Hand::Main,
                target,
                location,
            },
        );
        if outcome == Some(mod_api::ActionOutcome::Consumed) {
            return other;
        }
        for s in other {
            lv.spawn_stack(centre(pos), s, [0.0; 3]);
        }
        // `entity instanceof Player`: food it can eat, or a deployable drink, is finished on it.
        let player = lv
            .level
            .online_players(&*lv.host)
            .unwrap_or_default()
            .into_iter()
            .find(|p| p.entity() == target);
        if let Some(player) = player
            && feed(lv, pos, facing, d, player)
        {
            return Vec::new();
        }
    }
    let action = PlayerAction::UseOn {
        hand: Hand::Main,
        pos: reached,
        hit,
    };
    let (outcome, mut other) = act_with(lv, pos, facing, d, action);
    // Nothing used it: the held item's own use (`item.use`: buckets, bottles, snowballs...),
    // but an ender pearl's.
    let used = matches!(
        outcome,
        Some(mod_api::ActionOutcome::Consumed | mod_api::ActionOutcome::EffectBlocked(_))
    );
    let pearl = d
        .held
        .as_ref()
        .is_some_and(|(i, _)| i == "minecraft:ender_pearl");
    if !used && d.held.is_some() && !pearl {
        let (_, more) = act_with(
            lv,
            pos,
            facing,
            d,
            PlayerAction::UseItem { hand: Hand::Main },
        );
        other.extend(more);
    }
    other
}

/// A component of `stack`: its own (the patch), else its item's default.
fn component(host: &dyn mod_api::Host, stack: &ItemStack, name: &str) -> Option<mod_api::Tag> {
    stack
        .components()
        .and_then(|c| c.as_nbt().get(&name.into()).cloned())
        .or_else(|| host.item_default_component(stack.item(), name))
}

fn tag_number(t: Option<&mod_api::Tag>) -> Option<f64> {
    use mod_api::Tag;
    match t? {
        Tag::Byte(v) => Some(f64::from(*v)),
        Tag::Short(v) => Some(f64::from(*v)),
        Tag::Int(v) => Some(f64::from(*v)),
        Tag::Long(v) => Some(*v as f64),
        Tag::Float(v) => Some(f64::from(f32::from_bits(*v))),
        Tag::Double(v) => Some(f64::from_bits(*v)),
        _ => None,
    }
}

/// `activateInner` in `Mode.USE` on a player whose interaction passed: a held food the player
/// can eat (`canEat(canAlwaysEat)`), or a deployable drink (`create:deployable_drink`: milk,
/// potions), is finished on them (`stack.finishUsingItem(level, playerEntity)`), the hand keeping
/// what is left, and its crumbs fly at the hand (`spawnedItemEffects`). True when it was.
fn feed(
    lv: &mut ServerLevel<'_>,
    pos: [i32; 3],
    facing: Dir,
    d: &mut Deployer,
    target: PlayerRef,
) -> bool {
    let Some(stack) = held_of(lv, d) else {
        return false;
    };
    let host = &*lv.host;
    let food = match component(host, &stack, "minecraft:food") {
        Some(mod_api::Tag::Compound(c)) => Some((
            tag_number(c.get(&"nutrition".into())).unwrap_or(0.0) as i32,
            tag_number(c.get(&"saturation".into())).unwrap_or(0.0) as f32,
            tag_number(c.get(&"can_always_eat".into())).is_some_and(|v| v != 0.0),
        )),
        _ => None,
    };
    let eats = food.is_some_and(|(_, _, always)| target.can_eat(host, always) == Some(true));
    let drink = stacks::item_tagged(host, &stacks::id(host, &stack), "create:deployable_drink");
    if !eats && !drink {
        return false;
    }
    let left = finish_using(lv, &stack, target, food.filter(|_| eats));
    set_held_stack(&*lv.host, d, left.as_ref());
    // `SandPaperItem.spawnParticles` at the hand (`getMovementVector().scale(reach + 1)`): 20
    // crumbs of the stack as it was, each thrown up to 1/8 along each axis.
    if let Some(key) = ITEM_PARTICLE.get() {
        let o = facing.offset();
        let reach = f64::from(d.reach) + 1.0;
        let at = std::array::from_fn(|i| f64::from(pos[i]) + 0.5 + f64::from(o[i]) * reach);
        let options = mod_api::ParticleOptions::item_stack(*key, stack);
        lv.level.send_particles(
            &mut *lv.host,
            &options,
            None,
            at,
            [0.0; 3],
            0.125 / 3f64.sqrt(),
            20,
        );
    }
    true
}

/// `ItemStack.finishUsingItem(level, target)` for a consumable `stack` (`Consumable.onConsume`):
/// its sound, the food's listener (`FoodProperties.onConsume`: the eating sound, the food, a
/// burp), a potion's (`PotionContents.onConsume`) and a suspicious stew's effects, its consume
/// effects (milk's clearing), then one is used up unless the target has infinite materials, and
/// its `use_remainder` takes the emptied hand or goes to the target. What the hand keeps.
fn finish_using(
    lv: &mut ServerLevel<'_>,
    stack: &ItemStack,
    target: PlayerRef,
    food: Option<(i32, f32, bool)>,
) -> Option<ItemStack> {
    use mod_api::{SoundSource, Tag};
    let host = &*lv.host;
    let Some(Tag::Compound(consumable)) = component(host, stack, "minecraft:consumable") else {
        return Some(stack.clone());
    };
    let Some(living) = lv.host.living(target.entity()) else {
        return Some(stack.clone());
    };
    let at = target.entity().position(host).unwrap_or([0.0; 3]);
    let drinks = matches!(
        consumable.get(&"animation".into()),
        Some(Tag::String(a)) if a.to_string_lossy() == "drink"
    );
    let sound = match consumable.get(&"sound".into()) {
        Some(Tag::String(s)) => s.to_string_lossy(),
        Some(Tag::Compound(c)) => match c.get(&"sound_id".into()) {
            Some(Tag::String(s)) => s.to_string_lossy(),
            _ => String::new(),
        },
        _ => "minecraft:entity.generic.eat".into(),
    };
    let effects = match consumable.get(&"on_consume_effects".into()) {
        Some(Tag::List(l)) => l.len(),
        _ => 0,
    };
    // `Consumable.emit`: the consume sound at the target, from its own source.
    let (volume, pitch) = lv
        .level
        .with_random(&mut *lv.host, |r| {
            let eat_volume = if r.next_boolean() { 0.5 } else { 1.0 };
            let eat_pitch = 1.0 + 0.2 * (r.next_float() - r.next_float());
            let drink_pitch = 0.9 + r.next_float() * 0.1;
            if drinks {
                (0.5, drink_pitch)
            } else {
                (eat_volume, eat_pitch)
            }
        })
        .unwrap_or((1.0, 1.0));
    if !sound.is_empty() {
        lv.level.play_sound(
            &mut *lv.host,
            &sound,
            at,
            SoundSource::Players,
            volume,
            pitch,
            None,
        );
    }
    // `FoodProperties.onConsume`.
    if let Some((nutrition, saturation, _)) = food {
        let pitch = lv
            .level
            .with_random(&mut *lv.host, |r| {
                1.0 + 0.4 * (r.next_float() - r.next_float())
            })
            .unwrap_or(1.0);
        if !sound.is_empty() {
            lv.level.play_sound(
                &mut *lv.host,
                &sound,
                at,
                SoundSource::Neutral,
                1.0,
                pitch,
                None,
            );
        }
        // `FoodData.eat(nutrition, saturation)`, given as its modifier
        // (`saturation = nutrition * modifier * 2`).
        let modifier = if nutrition == 0 {
            0.0
        } else {
            saturation / (nutrition as f32 * 2.0)
        };
        target.feed(&mut *lv.host, nutrition, modifier);
        let pitch = lv
            .level
            .with_random(&mut *lv.host, |r| 0.9 + r.next_float() * 0.1)
            .unwrap_or(1.0);
        lv.level.play_sound(
            &mut *lv.host,
            "minecraft:entity.player.burp",
            at,
            SoundSource::Players,
            0.5,
            pitch,
            None,
        );
    }
    // `PotionContents.onConsume`.
    if let Some(t) = component(
        &*lv.host,
        stack,
        crate::glue::logistics::items::POTION_CONTENTS,
    ) {
        let contents = crate::glue::logistics::items::contents_of_tag(&t);
        crate::glue::fluids::drink_potion(lv, target.entity(), &contents);
    }
    // `SuspiciousStewEffects.onConsume`: each entry at amplifier 0.
    if let Some(Tag::List(stew)) = component(&*lv.host, stack, "minecraft:suspicious_stew_effects")
    {
        for e in stew {
            let Tag::Compound(e) = e else { continue };
            let Some(Tag::String(id)) = e.get(&"id".into()) else {
                continue;
            };
            let ticks = tag_number(e.get(&"duration".into())).unwrap_or(160.0) as i32;
            if let Some(key) = lv.host.effect_key(&id.to_string_lossy()) {
                let effect = mod_api::ActiveEffect::new(key, ticks, 0, false, true, true);
                living.add_effect(&mut *lv.host, &effect);
            }
        }
    }
    for i in 0..effects {
        let _ = living.apply_consume_effect(&mut *lv.host, stack, i);
    }
    // `stack.consume(1, entity)` and `UseRemainder.convertIntoRemainder`.
    if target.has_infinite_materials(&*lv.host) == Some(true) {
        return Some(stack.clone());
    }
    let left = stack.clone().shrink(1);
    let remainder = match component(&*lv.host, stack, "minecraft:use_remainder") {
        Some(Tag::Compound(c)) => match c.get(&"id".into()) {
            Some(Tag::String(id)) => stacks::stack(
                &*lv.host,
                &id.to_string_lossy(),
                tag_number(c.get(&"count".into())).unwrap_or(1.0) as u32,
            ),
            _ => None,
        },
        _ => None,
    };
    match (left, remainder) {
        (None, r) => r,
        (Some(left), Some(r)) => {
            let _ = target.give_stack(&mut *lv.host, r);
            Some(left)
        }
        (left, None) => left,
    }
}

/// The entities in the block `at`, but a contraption (`AbstractContraptionEntity`).
fn entities_at(lv: &ServerLevel<'_>, at: [i32; 3]) -> Vec<mod_api::EntityRef> {
    let lo = at.map(f64::from);
    let hi = at.map(|v| f64::from(v) + 1.0);
    lv.level
        .entities_in(&*lv.host, lo, hi)
        .unwrap_or_default()
        .into_iter()
        .filter(|e| {
            e.kind(&*lv.host)
                .map(|k| k.name(&*lv.host))
                .is_none_or(|k| !(k.starts_with("create:") && k.ends_with("contraption")))
        })
        .collect()
}

/// `level.random.nextInt(n)`.
fn random_index(lv: &mut ServerLevel<'_>, n: usize) -> usize {
    let n = n as i32;
    let pick = lv
        .level
        .with_random(&mut *lv.host, |r| r.next_int_bound(n))
        .unwrap_or(0) as usize;
    pick.min(n as usize - 1)
}

/// Where the fake player's eyes are and which way it looks (yaw, pitch) for the deployer at
/// `pos` facing `facing`. Create's fake player stands at the ray's origin with no eye height and
/// is not in the world; the game's has a standing body there (1.62 to its eyes), which would be
/// in the way of the block it places. It stands in the deployer instead: its eyes in the middle,
/// facing down above it, looking the way the deployer faces. For `use_item` the item's own ray
/// (a bucket's, a bottle's) starts where Create's does (`activateInner`'s ray origin, `3/2 + 1/64`
/// ahead, eyes on its feet), so it reaches the block two ahead and not the deployer's own pole.
fn fake_pose(pos: [i32; 3], facing: Dir, use_item: bool) -> ([f64; 3], f32, f32) {
    let mut eye = pos.map(|v| f64::from(v) + 0.5);
    if use_item {
        let o = facing.offset();
        eye = std::array::from_fn(|i| eye[i] + f64::from(o[i]) * (1.5 + 1.0 / 64.0));
    } else if facing == Dir::Down {
        eye[1] = f64::from(pos[1]) + 1.63;
    }
    let (yaw, pitch) = match facing {
        Dir::Down => (0.0, 90.0),
        Dir::Up => (0.0, -90.0),
        Dir::North => (180.0, 0.0),
        Dir::South => (0.0, 0.0),
        Dir::West => (90.0, 0.0),
        Dir::East => (-90.0, 0.0),
    };
    (eye, yaw, pitch)
}

/// Positions the fake player and equips its main hand for a direct punch action.
fn prepare_punch_player(
    lv: &mut ServerLevel<'_>,
    pos: [i32; 3],
    facing: Dir,
    d: &Deployer,
    stack: Option<ItemStack>,
) -> Option<PlayerRef> {
    let fake = fake_player(lv, d)?;
    let (eye, yaw, pitch) = fake_pose(pos, facing, false);
    let feet = [eye[0], eye[1] - 1.62, eye[2]];
    let host = &mut *lv.host;
    if !fake.entity().teleport_to(host, feet, Some([yaw, pitch])) {
        return None;
    }
    let living = host.living(fake.entity())?;
    living
        .set_hand_item(host, Hand::Main, stack)
        .ok()?
        .then_some(fake)
}

/// `Mode.PUNCH` on an entity (`DeployerHandler.activate`: `resetAttackStrengthTicker`, then
/// `player.attack(entity)`): the fake player stands in the deployer holding what `d` holds and
/// strikes `target` with `Player.attack`, so the attack hooks, the held weapon's damage,
/// enchantments and wear, knockback and sweep all run. The ticker is set far past the charge
/// time: `getCurrentItemAttackStrengthDelay` is 1/64 for Create's fake player, so its attacks
/// are always full, while the game's reads the held item's own. The hand keeps what is left of
/// the weapon.
fn attack(
    lv: &mut ServerLevel<'_>,
    pos: [i32; 3],
    facing: Dir,
    d: &mut Deployer,
    target: mod_api::EntityRef,
) {
    let stack = held_of(lv, d);
    let Some(fake) = prepare_punch_player(lv, pos, facing, d, stack) else {
        return;
    };
    let host = &mut *lv.host;
    let _ = fake.set_attack_strength_ticker(host, i32::MAX);
    let _ = fake.attack(host, target);
    let left = crate::glue::items::held(&*host, fake, Hand::Main);
    crate::glue::items::set_held(host, fake, Hand::Main, None);
    set_held_stack(&*host, d, left.as_ref());
}

/// The fake player at the deployer `pos` facing `facing` does `action` with what `d` holds: what
/// the action came to (None when the fake could not act) and what else the fake ended with; the
/// deployer keeps what is left in the hand.
fn act_with(
    lv: &mut ServerLevel<'_>,
    pos: [i32; 3],
    facing: Dir,
    d: &mut Deployer,
    action: PlayerAction,
) -> (Option<mod_api::ActionOutcome>, Vec<ItemStack>) {
    let Some(fake) = fake_player(lv, d) else {
        return (None, Vec::new());
    };
    let stack = held_of(lv, d);
    let (eye, yaw, pitch) = fake_pose(pos, facing, matches!(action, PlayerAction::UseItem { .. }));
    let setup = PlayerActionSetup::new(stack, eye, yaw, pitch, false);
    let Ok(result) = fake.perform_action_scoped(&mut *lv.host, action, setup) else {
        return (None, Vec::new());
    };
    set_held_stack(&*lv.host, d, result.held.as_ref());
    (Some(result.outcome), result.other)
}

/// `activateInner` in `Mode.PUNCH`: an entity in the block it reaches takes the hit (one at
/// random, any but a contraption); else, with a held stack, the block there breaks in strokes
/// (`Deployer::punch`), the fake player's cracks on it, a fire in front of it goes out; a
/// broken block is broken by the fake player with the held tool (its drops, the tool's wear).
fn punch(
    lv: &mut ServerLevel<'_>,
    pos: [i32; 3],
    facing: Dir,
    d: &mut Deployer,
    breaker: i32,
) -> Vec<ItemStack> {
    let at = deployer::clicked(pos, facing);
    let targets = entities_at(lv, at);
    if !targets.is_empty() {
        let target = targets[random_index(lv, targets.len())];
        attack(lv, pos, facing, d, target);
        return Vec::new();
    }
    // `getShape(..).isEmpty()`: nothing to punch, and the breaking is forgotten.
    if lv.shape(at, mod_api::BlockShapeKind::Outline).is_empty() {
        d.breaking = None;
        return Vec::new();
    }
    // `BlockHelper.extinguishFire`: fire on the face toward the deployer goes out.
    let front = super::step(at, deployer::clicked_face(facing), 1);
    if lv.block_name(front).as_deref() == Some("minecraft:fire") {
        let _ = lv.level.level_event(&mut *lv.host, 1009, front, 0);
        let _ = lv.level.remove_block(&mut *lv.host, front, false);
        return Vec::new();
    }
    // A stack whose saved components cannot be restored is no stack to dig with: the stroke is
    // skipped rather than run with a plain substitute.
    let Some(Ok(stack)) = d
        .held_stack()
        .and_then(|h| rebuild(&h, |id, n| stacks::stack(&*lv.host, id, n)))
    else {
        return Vec::new();
    };
    let Some(state) = lv.block_state(at) else {
        return Vec::new();
    };
    // `DeployerHandler.activateInner`: `getDestroyProgress(player, ..)` of the fake player
    // holding the whole stack, its components and wear included.
    let Some(fake) = prepare_punch_player(lv, pos, facing, d, Some(stack)) else {
        return Vec::new();
    };
    let host = &mut *lv.host;
    let per_hit = fake.destroy_progress(host, lv.level, at, state);
    let Some(living) = host.living(fake.entity()) else {
        return Vec::new();
    };
    if !matches!(living.set_hand_item(host, Hand::Main, None), Ok(true)) {
        return Vec::new();
    }
    let Some(per_hit) = per_hit else {
        return Vec::new();
    };
    // `level.playSound(.., getHitSound(), NEUTRAL, .25, 1)`.
    if let Some(sound) = mod_api::HostBlockStates::block_states(&*lv.host).sound(state) {
        let _ = lv.level.play_sound(
            &mut *lv.host,
            sound.hit.as_str(),
            centre(at),
            mod_api::SoundSource::Neutral,
            0.25,
            1.0,
            None,
        );
    }
    match d.punch(at, per_hit) {
        deployer::Punch::Broken => {
            let other = harvest(lv, pos, facing, d, at);
            let _ = lv.level.set_break_progress(&mut *lv.host, breaker, at, -1);
            return other;
        }
        deployer::Punch::Crack(Some(stage)) => {
            let _ = lv
                .level
                .set_break_progress(&mut *lv.host, breaker, at, stage);
        }
        deployer::Punch::Crack(None) | deployer::Punch::Stop => {}
    }
    Vec::new()
}

/// `DeployerHandler.tryHarvestBlock` for the fake player at the deployer `pos` facing `facing`,
/// holding what `d` holds: the block at `at` breaks as it breaks it
/// (`ServerPlayerGameMode.destroyBlock`: the held tool's wear and the drops it harvests), the
/// drops into its inventory (`placeItemBackInInventory`). The hand keeps what is left of the
/// tool; what else the fake player ended with.
fn harvest(
    lv: &mut ServerLevel<'_>,
    pos: [i32; 3],
    facing: Dir,
    d: &mut Deployer,
    at: [i32; 3],
) -> Vec<ItemStack> {
    use crate::glue::owned::OwnedInventory as _;
    let stack = held_of(lv, d);
    let Some(fake) = prepare_punch_player(lv, pos, facing, d, stack) else {
        return Vec::new();
    };
    let host = &mut *lv.host;
    fake.destroy_block(host, at, true);
    let slots = fake.inventory_slots(&*host).unwrap_or_default();
    let mut other = Vec::new();
    let mut held = None;
    for (slot, s) in slots {
        let _ = fake.set_inventory_slot(host, slot, None);
        if slot == 0 {
            held = Some(s);
        } else {
            other.push(s);
        }
    }
    set_held_stack(&*host, d, held.as_ref());
    other
}

/// `BeehiveBlockMixin`: a deployer harvesting a full beehive or bee nest (`honey_level` 5)
/// counts as smoked — honey is taken, `honey_level` resets to 0 and the bees stay calm.
/// Returns true when the event was handled and cancelled with `InteractionResult::Success`.
pub fn calm_hive_harvest(
    host: &mut dyn ServerHost,
    event: &mut mod_api::RightClickBlockEvent<'_>,
) -> bool {
    let click = event.click;
    if *event.use_block == mod_api::TriState::False
        || host
            .players()
            .acting_player()
            .is_none_or(|p| p.is_fake(&*host) != Some(true))
    {
        return false;
    }
    let deployers = with_world(|w| {
        w.dims.iter().any(|d| {
            d.deployers
                .values()
                .any(|dp| dp.fake_uuid() == click.player)
        })
    });
    if deployers != Some(true) {
        return false;
    }
    let Some(stack) = click.stack else {
        return false;
    };
    let item = stacks::id(&*host, stack);
    if !matches!(item.as_str(), "minecraft:shears" | "minecraft:glass_bottle") {
        return false;
    }
    let dimension = click.dimension.key().id().to_owned();
    let Some(mut lv) = ServerLevel::new(host, &dimension) else {
        return false;
    };
    let pos = click.pos;
    let Some(state) = lv.block_state(pos) else {
        return false;
    };
    let Some(block) = lv.block_name(pos) else {
        return false;
    };
    let Some(honey_level) = lv
        .property(pos, "honey_level")
        .and_then(|v| v.parse::<u8>().ok())
    else {
        return false;
    };
    let Some(harvest) = deployer::hive_harvest(&block, honey_level, &item) else {
        return false;
    };
    let Ok(empty_hive) = state.with(&lv.host.block_states(), "honey_level", "0") else {
        return false;
    };
    if empty_hive == state {
        return false;
    }
    let Some(player) = lv.host.players().acting_player() else {
        return false;
    };
    let context = mod_api::GameEventContext {
        source_entity: Some(player.entity()),
        ..Default::default()
    };
    match harvest {
        deployer::HiveHarvest::Shear => {
            lv.play_block_sound("minecraft:block.beehive.shear", pos, 1.0, 1.0);
            if let Ok(Some(drops)) = lv.level.roll_loot(
                &mut *lv.host,
                "minecraft:harvest/beehive",
                centre(pos),
                Some(stack),
                Some(player.entity()),
            ) {
                for drop in drops {
                    let _ = lv.level.pop_resource(&mut *lv.host, pos, drop);
                }
            }
            if let Some(living) = lv.host.living(player.entity()) {
                let slot = match click.hand {
                    Hand::Main => mod_api::EquipmentSlot::MainHand,
                    Hand::Off => mod_api::EquipmentSlot::OffHand,
                };
                living.hurt_and_break(&mut *lv.host, slot, 1);
            }
            lv.level
                .game_event(&mut *lv.host, "minecraft:shear", centre(pos), context);
        }
        deployer::HiveHarvest::Bottle => {
            let Some(honey) = stacks::stack(&*lv.host, "minecraft:honey_bottle", 1) else {
                return false;
            };
            lv.level.play_sound(
                &mut *lv.host,
                "minecraft:item.bottle.fill",
                centre(pos),
                mod_api::SoundSource::Neutral,
                1.0,
                1.0,
                None,
            );
            match stack.clone().shrink(1) {
                None => {
                    crate::glue::items::set_held(&mut *lv.host, player, click.hand, Some(honey));
                }
                Some(rest) => {
                    crate::glue::items::set_held(&mut *lv.host, player, click.hand, Some(rest));
                    if let Ok(Some(overflow)) = player.give_stack(&mut *lv.host, honey) {
                        let _ = lv.level.pop_resource(&mut *lv.host, pos, overflow);
                    }
                }
            }
            lv.level.game_event(
                &mut *lv.host,
                "minecraft:fluid_pickup",
                centre(pos),
                context,
            );
        }
    }
    lv.set_state(pos, empty_hive, crate::glue::server::update_all());
    *event.cancellation_result = mod_api::InteractionResult::Success;
    true
}

/// How far along its facing a click at `point` is from the deployer's back face.
fn along(pos: [i32; 3], facing: Dir, point: [f64; 3]) -> f64 {
    let o = facing.offset();
    (0..3)
        .map(|i| (point[i] - (f64::from(pos[i]) + 0.5 - 0.5 * f64::from(o[i]))) * f64::from(o[i]))
        .sum::<f64>()
}

/// `DeployerBlock.onWrenched`: the wrench on its front half (more than .75 from its back along
/// the facing, at `point`) toggles using and punching (`changeMode`); true when it did.
pub fn wrench_deployer(lv: &mut ServerLevel<'_>, pos: [i32; 3], point: [f64; 3]) -> bool {
    let dim = lv.dim;
    let Some(facing) = lv.facing_of(pos, Block::Deployer) else {
        return false;
    };
    if along(pos, facing, point).abs() <= 0.75 {
        return false;
    }
    with_world(|w| {
        if let Some(mut d) = w.dims[dim].deployers.get(&pos).cloned() {
            d.mode = d.mode.toggled();
            w.set_deployer(dim, pos, d);
        }
    });
    true
}

/// `DeployerBlock.useItemOn`'s placement helper (`DeployerBlock.PlacementHelper`,
/// `PlacementOffset.placeInWorld`): a deployer item, not sneaking, by a player who may build,
/// goes on the free side of the deployer at `pos` nearest the click
/// ([`deployer::helper_offset`]), as that one stands; its place sound, one used up outside
/// creative. True when it was placed.
pub fn place_beside(
    lv: &mut ServerLevel<'_>,
    pos: [i32; 3],
    facing: Dir,
    click: &BlockClick<'_>,
    player: PlayerRef,
) -> bool {
    let Some(stack) = click.stack else {
        return false;
    };
    if stacks::id(&*lv.host, stack) != "create:deployer"
        || player.entity().is_shift_key_down(&*lv.host) == Some(true)
        || player.may_build(&*lv.host) != Some(true)
    {
        return false;
    }
    let hit = std::array::from_fn(|i| click.hit.point[i] - f64::from(pos[i]) - 0.5);
    let Some(at) = deployer::helper_offset(pos, facing, hit, |p| lv.replaceable(p)) else {
        return false;
    };
    let Some(state) = lv.block_state(pos) else {
        return false;
    };
    let level = lv.level;
    let face = match Dir::ALL.into_iter().find(|d| super::step(pos, *d, 1) == at) {
        Some(Dir::Down) => mod_api::BlockFace::Down,
        Some(Dir::North) => mod_api::BlockFace::North,
        Some(Dir::South) => mod_api::BlockFace::South,
        Some(Dir::West) => mod_api::BlockFace::West,
        Some(Dir::East) => mod_api::BlockFace::East,
        _ => mod_api::BlockFace::Up,
    };
    if player
        .place_block(&mut *lv.host, level, at, state, face, true)
        .is_err()
    {
        return false;
    }
    lv.place_sound(at);
    if player.is_creative(&*lv.host) != Some(true)
        && let Some(living) = lv.host.living(player.entity())
    {
        let _ = living.set_hand_item(&mut *lv.host, click.hand, stack.clone().shrink(1));
    }
    true
}

/// `DeployerBlock.useItemOn`: a click on its front half swaps the player's hand with the
/// deployer's. True when the click was the deployer's.
pub fn use_deployer(
    lv: &mut ServerLevel<'_>,
    pos: [i32; 3],
    facing: Dir,
    click: &BlockClick<'_>,
    player: PlayerRef,
    main: bool,
) -> bool {
    let dim = lv.dim;
    let host = &*lv.host;
    if click
        .stack
        .is_some_and(|h| stacks::id(host, h) == "create:wrench")
    {
        return false;
    }
    // The hit along the facing from the back face: less than .75 is the back's own use.
    if along(pos, facing, click.hit.point).abs() < 0.75 {
        return false;
    }
    if !main {
        return true;
    }
    let hand = lv.held(player);
    let Some(mut d) = with_world(|w| w.dims[dim].deployers.get(&pos).cloned()).flatten() else {
        return true;
    };
    if d.held.is_none() && hand.is_none() {
        return true;
    }
    let back = held_of(lv, &d);
    set_held_stack(&*lv.host, &mut d, hand.as_ref());
    lv.set_held(player, back);
    with_world(|w| w.set_deployer(dim, pos, d));
    true
}

/// Deployers' strokes to `players`: what runs the hand and what it holds, as each deployer
/// stands (`write` with a client packet); one gone to take its hand away.
fn send_deployers(lv: &mut ServerLevel<'_>, players: &[PlayerRef], list: &[[i32; 3]]) {
    let dim = lv.dim;
    let mut shown = Vec::new();
    for &pos in list {
        let facing = lv.facing_of(pos, Block::Deployer);
        let d = with_world(|w| w.dims[dim].deployers.get(&pos).cloned()).flatten();
        shown.push(deployer::Shown {
            pos,
            facing: facing.unwrap_or(Dir::Up),
            deployer: d,
        });
    }
    for chunk in shown.chunks(64) {
        let m = deployer::encode(chunk);
        net::send_to_players(lv.host, players, deployer::CHANNEL, &m);
    }
}

/// A player joined: every deployer of their level.
pub fn player_joined(host: &mut dyn ServerHost, player: PlayerRef) {
    let Some(dimension) = player
        .level(&*host)
        .and_then(|l| l.dimension().id(&*host).map(str::to_owned))
    else {
        return;
    };
    let Some(mut lv) = ServerLevel::new(host, &dimension) else {
        return;
    };
    let dim = lv.dim;
    let deployers: Vec<[i32; 3]> =
        with_world(|w| w.dims[dim].deployers.keys().copied().collect()).unwrap_or_default();
    if !deployers.is_empty() {
        send_deployers(&mut lv, &[player], &deployers);
    }
}
