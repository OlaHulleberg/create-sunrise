//! The deployers' contract (`deployers.json`): the blocks a deployer reaches (full states), how
//! far `start`'s ray runs before each stops it, the items it holds and what the game does with them (`DeployerHandler`: a block item places its
//! block, an axe, a shovel or a hoe turns the block it is used on), what a punch with each held
//! stack does on each block (`getDestroyProgress` from the tool's speed and whether it harvests,
//! the drops), and Create's deploying, item application and sandpaper polishing recipes for the
//! pairs the family reaches. The server audits every entry at startup (`DeployerAudit`).
//! Its histories are `generate`.
use crate::{
    actions::{Action, Face},
    generate::Rng,
    scenario::{Change, Placed, Pos, Scenario},
    transport::{Input, Stack},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::OnceLock;

#[derive(Deserialize)]
pub struct Contract {
    states: Vec<String>,
    pub items: Vec<Item>,
    pub blocks: Vec<Block>,
    start_rays: Vec<StartRay>,
    dig: Vec<Dig>,
    recipes: Recipes,
}

#[derive(Deserialize, Clone)]
pub struct Item {
    pub id: String,
    pub max_stack: u8,
    #[serde(default)]
    pub max_damage: u32,
    #[serde(default)]
    pub remainder: Option<String>,
    /// The state its block item places (`BlockItem.getBlock`).
    #[serde(default)]
    pub block: Option<String>,
    /// An axe, a shovel or a hoe: the block it is used on (by name) and the state it becomes.
    #[serde(default)]
    pub turns: BTreeMap<String, String>,
    /// Whether it also turns a block clicked from below (an axe's and a hoe's use does, a
    /// shovel's passes).
    #[serde(default = "yes")]
    pub turns_from_below: bool,
}

fn yes() -> bool {
    true
}

/// A block a deployer reaches: when a punch can break it, its hardness (`getDestroySpeed`).
#[derive(Deserialize, Clone)]
pub struct Block {
    pub state: String,
    #[serde(default)]
    pub hardness: Option<f32>,
}

/// How far `DeployerBlockEntity.start`'s ray (`deployer::start_ray`) runs before `state`, two
/// ahead of the deployer, stops it, facing down, up and east (the others as east). One ahead it
/// never does.
#[derive(Deserialize, Clone)]
struct StartRay {
    state: String,
    down: f64,
    up: f64,
    east: f64,
}

/// A punch at the block `block` with `held`: the fake player's destroy speed, whether it
/// harvests the block (`canHarvestBlock`), and what `tryHarvestBlock` collects.
#[derive(Deserialize, Clone)]
pub struct Dig {
    pub held: String,
    pub block: String,
    pub speed: f32,
    pub harvest: bool,
    pub drops: Vec<Stack>,
}

impl Dig {
    /// `BlockBehaviour.getDestroyProgress` for the fake player on a block of `hardness`, as
    /// `DeployerAudit` checks it: the tool's speed over five, over the hardness and 30 when it
    /// harvests, else 100; 0 for an unbreakable block.
    pub fn progress(&self, hardness: f32) -> f32 {
        if hardness == -1.0 {
            return 0.0;
        }
        self.speed / 5.0 / hardness / if self.harvest { 30.0 } else { 100.0 }
    }
}

#[derive(Deserialize, Clone)]
pub struct Recipe {
    pub id: String,
    pub target: Vec<String>,
    pub tool: Vec<String>,
    pub results: Vec<Output>,
    #[serde(default)]
    pub keep_held: bool,
}

#[derive(Deserialize, Clone)]
pub struct Output {
    pub item: String,
    pub count: u8,
    pub chance: f32,
}

#[derive(Deserialize)]
struct Recipes {
    polishing: Vec<Recipe>,
    deploying: Vec<Recipe>,
    application: Vec<Recipe>,
}

pub fn contract() -> &'static Contract {
    static C: OnceLock<Contract> = OnceLock::new();
    C.get_or_init(|| {
        let c: Contract =
            serde_json::from_str(include_str!("deployers.json")).expect("deployer contract");
        for i in &c.items {
            assert!(
                crate::transport::contract()
                    .items
                    .iter()
                    .any(|t| t.id == i.id),
                "deployer contract: {} is not a transport item",
                i.id
            );
            if let Some(b) = &i.block
                && b.starts_with("minecraft:")
                && !c.states.contains(b)
            {
                panic!("deployer contract: {b} is not a state");
            }
            for (from, to) in &i.turns {
                assert!(
                    c.states
                        .iter()
                        .any(|s| crate::scenario::parse_state(s).0 == *from),
                    "deployer contract: {from} is not a state"
                );
                assert!(
                    c.states
                        .iter()
                        .any(|s| crate::scenario::parse_state(s).0 == *to),
                    "deployer contract: {to} is not a state"
                );
            }
        }
        for b in &c.blocks {
            assert!(
                c.states.contains(&b.state),
                "deployer contract: {}",
                b.state
            );
        }
        c
    })
}

impl Contract {
    pub fn item(&self, id: &str) -> Option<&Item> {
        self.items.iter().find(|i| i.id == id)
    }
    /// The block of `state` in the contract: its own state, else the first of its block.
    pub fn block(&self, state: &str) -> Option<&Block> {
        self.blocks.iter().find(|b| b.state == state).or_else(|| {
            let name = crate::scenario::parse_state(state).0;
            self.blocks
                .iter()
                .find(|b| crate::scenario::parse_state(&b.state).0 == name)
        })
    }
    /// [`StartRay`] for `state` (its own, else the first of its block) with the deployer facing
    /// `facing`.
    pub fn start_hit(&self, state: &str, facing: create_engine::kinetics::Dir) -> Option<f64> {
        use create_engine::kinetics::Dir;
        let name = crate::scenario::parse_state(state).0;
        let r = self
            .start_rays
            .iter()
            .find(|r| r.state == state)
            .or_else(|| {
                self.start_rays
                    .iter()
                    .find(|r| crate::scenario::parse_state(&r.state).0 == name)
            })?;
        Some(match facing {
            Dir::Down => r.down,
            Dir::Up => r.up,
            _ => r.east,
        })
    }
    pub fn dig(&self, held: &str, block: &str) -> Option<&Dig> {
        self.dig.iter().find(|d| d.held == held && d.block == block)
    }
    /// The recipes `DeployerBlockEntity.getRecipe` looks at with sand paper (`SANDPAPER_POLISHING`).
    pub fn polishing(&self) -> &[Recipe] {
        &self.recipes.polishing
    }
    /// `create:deploying`, then the automatable `create:item_application`: the port's
    /// `Recipes.deploying`.
    pub fn deploying(&self) -> impl Iterator<Item = &Recipe> {
        self.recipes
            .deploying
            .iter()
            .chain(self.recipes.application.iter())
    }
    /// `create:item_application` for a click on a block (`ManualApplicationRecipe`), the
    /// manual-only ones too.
    pub fn application(&self) -> &[Recipe] {
        &self.recipes.application
    }
}

/// `create_engine::deployer::Recipe` of `r`.
pub fn engine_recipe(r: &Recipe) -> create_engine::deployer::Recipe {
    create_engine::deployer::Recipe {
        id: r.id.clone(),
        target: r.target.clone(),
        tool: r.tool.clone(),
        results: r
            .results
            .iter()
            .map(|o| create_engine::processing::Output {
                item: o.item.clone(),
                count: o.count,
                chance: o.chance,
            })
            .collect(),
        keep_held: r.keep_held,
    }
}

pub fn item(id: &str) -> Option<&'static Item> {
    contract().item(id)
}

/// Whether the contract names the whole state `state` as one a deployer may reach (Create's
/// own casings among them, which the kinetic catalogue does not list).
pub fn names_state(state: &str) -> bool {
    contract().states.iter().any(|s| s == state)
}

/// The block an item's block item places, None for a plain item.
pub fn item_block(id: &str) -> Option<&'static str> {
    item(id)?.block.as_deref()
}

/// The states of the contract the engine's table does not hold: below `create-fuzz`'s first
/// Create state and above the fluid world's.
pub const FIRST_STATE: u32 = 170;
const AIR: &str = "minecraft:air";
const STONE: &str = "minecraft:stone";

pub fn states() -> &'static [String] {
    static S: OnceLock<Vec<String>> = OnceLock::new();
    S.get_or_init(|| {
        let c = contract();
        let mut out: Vec<String> = Vec::new();
        let mut push = |s: &str| {
            let (name, props) = crate::scenario::parse_state(s);
            let s = crate::scenario::format_state(&name, &props);
            if s != AIR && s != STONE && !out.contains(&s) && !s.starts_with("create:") {
                out.push(s);
            }
        };
        for s in &c.states {
            push(s);
        }
        for i in &c.items {
            if let Some(b) = &i.block {
                push(b);
            }
        }
        for i in &c.items {
            for to in i.turns.values() {
                push(to);
            }
        }
        for b in &c.blocks {
            push(&b.state);
        }
        assert!(
            out.len() < (256 - FIRST_STATE) as usize,
            "the deployer contract's states exceed their id range"
        );
        out
    })
}

/// The driver's id of a contract state, None for any other.
pub fn id(state: &str) -> Option<u32> {
    let (name, props) = crate::scenario::parse_state(state);
    let state = crate::scenario::format_state(&name, &props);
    states()
        .iter()
        .position(|s| *s == state)
        .map(|i| FIRST_STATE + i as u32)
}

pub fn name(id: u32) -> Option<String> {
    states().get(id.checked_sub(FIRST_STATE)? as usize).cloned()
}

/// Whether the contract names the block (by the name of its state), the harness's stone too.
pub fn known_block(block: &str) -> bool {
    block == STONE
        || block == "create:deployer"
        || contract()
            .states
            .iter()
            .any(|s| crate::scenario::parse_state(s).0 == block)
        || contract()
            .items
            .iter()
            .any(|i| i.block.as_deref() == Some(block))
}

/// Whether `state` is one of the contract's vanilla blocks (not air or stone).
pub fn vanilla(state: &str) -> bool {
    crate::scenario::parse_state(state)
        .0
        .starts_with("minecraft:")
        && id(state).is_some()
}

/// The item's stack size: the transport contract's, else the deployer contract's.
pub fn limit(item: &str) -> Option<u8> {
    crate::transport::contract()
        .items
        .iter()
        .find(|i| i.id == item)
        .map(|i| i.max_stack)
        .or_else(|| contract().item(item).map(|i| i.max_stack))
}

/// `DeployerFilterSlot`: the centre of a deployer's filter slot on `side`, in the block: 7.5 px
/// out of a side off the facing's and the shaft's axes, 2 px back from the facing; None on a
/// side of either axis.
pub fn filter_slot(
    facing: create_engine::kinetics::Dir,
    along_first: bool,
    side: create_engine::kinetics::Dir,
) -> Option<[f64; 3]> {
    let shaft = create_engine::parts::directional_axis(facing, along_first);
    if side.axis() == facing.axis() || side.axis() == shaft {
        return None;
    }
    let o = side.offset();
    let f = facing.offset();
    Some(std::array::from_fn(|i| {
        0.5 + f64::from(o[i]) * 7.5 / 16.0 - f64::from(f[i]) * 2.0 / 16.0
    }))
}

/// `BlockHelper.copyProperties`: the block `to` as the contract's state of it, with the values
/// of the properties it shares with `from` (a stripped log keeps its axis).
pub fn turned(from: &str, to: &str) -> Option<String> {
    let contract = contract();
    let base = contract
        .states
        .iter()
        .find(|s| crate::scenario::parse_state(s).0 == to)?;
    let (_, mut props) = crate::scenario::parse_state(base);
    let (_, old) = crate::scenario::parse_state(from);
    for (k, v) in props.iter_mut() {
        if let Some((_, ov)) = old.iter().find(|(ok, _)| ok == k) {
            *v = ov.clone();
        }
    }
    Some(crate::scenario::format_state(to, &props))
}

/// The state a block item of the contract places when clicked on `clicked`: a pillar's axis
/// (`axis`) turns with the clicked face (`RotatedPillarBlock.getStateForPlacement`).
pub fn placed_state(placed: &str, clicked: create_engine::kinetics::Dir) -> String {
    let (block, props) = crate::scenario::parse_state(placed);
    let mut props = props;
    if props.iter().any(|(k, _)| k == "axis") {
        let axis = match clicked.axis() {
            create_engine::kinetics::Axis::X => "x",
            create_engine::kinetics::Axis::Y => "y",
            create_engine::kinetics::Axis::Z => "z",
        };
        for (k, v) in props.iter_mut() {
            if k == "axis" {
                *v = axis.into();
            }
        }
    }
    crate::scenario::format_state(&block, &props)
}

/// A deployer's stack: an item, its count and its damage (`ItemStack.getDamageValue`).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Held {
    pub item: String,
    pub count: u8,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub damage: u32,
}

impl Held {
    pub fn new(item: &str, count: u8) -> Held {
        Held {
            item: item.into(),
            count,
            damage: 0,
        }
    }
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// The block a deployer's fake player is breaking, and how far (`blockBreakingProgress`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Breaking {
    pub pos: Pos,
    pub progress: f32,
}

/// A deployer as both sides see it: `DeployerBlockEntity` and its fake player.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Observation {
    pub kind: String,
    /// `Mode`: `use` or `punch`.
    pub mode: String,
    /// `State`.
    pub state: String,
    pub timer: i32,
    pub reach: f32,
    /// The fake player's main hand.
    pub held: Option<Held>,
    /// What else the fake player's inventory holds.
    pub inventory: Vec<Held>,
    /// `overflowItems`.
    pub overflow: Vec<Held>,
    /// `redstoneLocked`.
    pub locked: bool,
    /// `Owner`: a player's UUID (the port keeps none).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// `FilteringBehaviour.getFilter`'s item.
    pub filter: Option<String>,
    pub fist_bump: bool,
    pub breaking: Option<Breaking>,
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1e-3 * a.abs().max(b.abs()).max(1.)
}

pub fn equal(a: Option<&Observation>, b: Option<&Observation>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.kind == b.kind
                && a.mode == b.mode
                && a.state == b.state
                && a.timer == b.timer
                && close(a.reach, b.reach)
                && a.held == b.held
                && a.inventory == b.inventory
                && a.overflow == b.overflow
                && a.locked == b.locked
                && a.owner == b.owner
                && a.filter == b.filter
                && a.fist_bump == b.fist_bump
                && match (&a.breaking, &b.breaking) {
                    (None, None) => true,
                    (Some(a), Some(b)) => a.pos == b.pos && close(a.progress, b.progress),
                    _ => false,
                }
        }
        _ => false,
    }
}

fn placed(pos: Pos, state: &str, motor: Option<i32>) -> Placed {
    Placed {
        pos,
        state: state.into(),
        motor,
    }
}

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

fn drop(item: &str, count: u8) -> Action {
    Action::Item {
        input: Input::DropOn {
            stack: Stack {
                item: item.into(),
                count,
            },
        },
    }
}

/// A player's use of `item` on the `face` of the block at `pos`, hit at `hit` in it.
fn use_on(tick: u32, pos: Pos, item: &str, face: Face, hit: [f64; 3]) -> Change {
    act(
        tick,
        pos,
        Action::Use {
            item: item.into(),
            face,
            hit,
            yaw: 0.,
            pitch: if face == Face::Up { 90. } else { 0. },
            sneaking: false,
        },
    )
}

/// `create:deployer[axis_along_first=..,facing=..]`.
fn deployer_state(facing: Face, along_first: bool) -> String {
    format!(
        "create:deployer[axis_along_first={along_first},facing={:?}]",
        facing.dir()
    )
    .to_lowercase()
}

/// A deployer at `at` facing `facing`, and the motor behind its shaft.
struct Rig {
    at: Pos,
    facing: Face,
    along_first: bool,
    motor: Pos,
    motor_state: &'static str,
}

impl Rig {
    fn new(at: Pos, facing: Face, along_first: bool) -> Rig {
        use create_engine::kinetics::Axis;
        let axis = create_engine::parts::directional_axis(facing.dir(), along_first);
        let (motor, motor_state) = match axis {
            Axis::X => (
                [at[0] - 1, at[1], at[2]],
                "create:creative_motor[facing=east]",
            ),
            Axis::Y => (
                [at[0], at[1] - 1, at[2]],
                "create:creative_motor[facing=up]",
            ),
            Axis::Z => (
                [at[0], at[1], at[2] - 1],
                "create:creative_motor[facing=south]",
            ),
        };
        Rig {
            at,
            facing,
            along_first,
            motor,
            motor_state,
        }
    }

    /// The deployer's blocks: the motor, turning at `rpm`, and the deployer.
    fn build(&self, sc: &mut Scenario, rpm: i32) {
        sc.blocks
            .push(placed(self.motor, self.motor_state, Some(rpm)));
        sc.blocks.push(placed(
            self.at,
            &deployer_state(self.facing, self.along_first),
            None,
        ));
    }

    /// The block `n` along its facing (`worldPosition.relative(FACING, n)`).
    fn ahead(&self, n: i32) -> Pos {
        let o = self.facing.dir().offset();
        std::array::from_fn(|i| self.at[i] + o[i] * n)
    }

    /// The face toward what it reaches, at its middle: a click there changes its mode by
    /// wrench (`onWrenched`), otherwise swaps its hand (`useItemOn`).
    fn front(&self) -> (Face, [f64; 3]) {
        (self.facing, self.facing.point(0.5, 0.5))
    }

    /// A side holding its filter slot, and the hit on it.
    fn filter_side(&self) -> Option<(Face, [f64; 3])> {
        let sides = [
            Face::North,
            Face::South,
            Face::West,
            Face::East,
            Face::Up,
            Face::Down,
        ];
        sides.into_iter().find_map(|side| {
            let slot = filter_slot(self.facing.dir(), self.along_first, side.dir())?;
            let o = side.dir().offset();
            let hit = std::array::from_fn(|i| {
                if o[i] == 0 {
                    slot[i]
                } else if o[i] > 0 {
                    1.0
                } else {
                    0.0
                }
            });
            Some((side, hit))
        })
    }

    fn give(&self, tick: u32, item: &str) -> Change {
        let (face, hit) = self.front();
        use_on(tick, self.at, item, face, hit)
    }

    /// The wrench on its front half: using and punching change places (`changeMode`).
    fn wrench(&self, tick: u32) -> Change {
        self.give(tick, "create:wrench")
    }

    /// The block it reaches, set to `state`.
    fn set_ahead(&self, tick: u32, n: i32, state: &str) -> Change {
        Change::set(tick, self.ahead(n), state)
    }
}

const DEPOT: &str = "create:depot[waterlogged=false]";
const INGOT: &str = "minecraft:iron_ingot";

/// `(target, tool)`: the pairs a stack under a deployer and its held stack come in.
const PAIRS: [(&str, &str); 11] = [
    ("create:shaft", "minecraft:oak_planks"),
    ("create:cogwheel", "minecraft:oak_planks"),
    ("minecraft:exposed_copper", "minecraft:iron_axe"),
    ("minecraft:exposed_copper", "minecraft:wooden_axe"),
    ("minecraft:copper_block", "minecraft:honeycomb_block"),
    ("minecraft:waxed_copper_block", "minecraft:iron_axe"),
    ("minecraft:stripped_oak_log", "create:andesite_alloy"),
    ("minecraft:stripped_oak_log", "create:brass_ingot"),
    ("minecraft:stripped_oak_log", "minecraft:copper_ingot"),
    ("create:rose_quartz", "create:sand_paper"),
    // No recipe: the stack passes.
    ("minecraft:dirt", "minecraft:oak_planks"),
];

/// The last tick of every history.
const TICKS: u32 = 470;

fn base(seed: u64) -> Scenario {
    Scenario {
        version: 3,
        seed,
        ticks: TICKS,
        day_time: 0,
        checkpoints: vec![],
        blocks: vec![],
        events: vec![],
    }
}

/// A locked deployer lets stacks pass and starts nothing: a signal beside it, then gone.
fn lock(sc: &mut Scenario, rig: &Rig, tick: u32, r: &mut Rng) {
    let beside = [
        if rig.facing.dir().offset()[0] != 0 {
            0
        } else {
            1
        },
        rig.at[1],
        rig.at[2],
    ];
    sc.events.push(act(
        tick,
        beside,
        Action::Signal {
            strength: 1 + r.below(14) as u8,
        },
    ));
    let clear = tick + 20 + r.below(60);
    sc.events.push(Change::set(clear, beside, AIR));
    sc.checkpoints
        .extend((tick..tick + 4).chain(clear..clear + 4));
}

/// Stacks under the deployer for its held stack, each a while apart; the checkpoints around
/// their arrival.
fn stacks(sc: &mut Scenario, r: &mut Rng, under: Pos, pair: (&str, &str), first: u32) {
    let n = 2 + r.below(3);
    let mut tick = first;
    for _ in 0..n {
        sc.events
            .push(act(tick, under, drop(pair.0, 1 + r.below(12) as u8)));
        sc.checkpoints.extend(tick - 1..tick + 7);
        tick += 80 + r.below(100);
    }
}

/// A deployer facing down over a depot: its recipes, a punch mode that lets stacks pass, a
/// redstone lock, the hand taken back, and the outputs taken.
fn over_depot(sc: &mut Scenario, r: &mut Rng, rpm: i32, filter: bool) {
    let depot = [0, 8, 0];
    let rig = Rig::new([0, 10, 0], Face::Down, false);
    sc.blocks.push(placed(depot, DEPOT, None));
    rig.build(sc, rpm);
    let pair = if filter {
        ("create:shaft", "minecraft:oak_planks")
    } else {
        *r.pick(&PAIRS)
    };
    if filter {
        // A filter naming its tool lets it work; one naming another item puts the tool away.
        let named = if r.chance(0.5) { INGOT } else { pair.1 };
        let (side, hit) = rig
            .filter_side()
            .expect("a slot beside a downward deployer");
        sc.events.push(use_on(8, rig.at, named, side, hit));
        if named == INGOT {
            sc.events.push(use_on(150, rig.at, AIR, side, hit));
        }
    }
    sc.events.push(rig.give(12, pair.1));
    stacks(sc, r, depot, pair, 34);
    if r.chance(0.4) {
        let t = 60 + r.below(160);
        sc.events.push(rig.wrench(t));
        if r.chance(0.5) {
            sc.events.push(rig.wrench(t + 30 + r.below(60)));
        }
    }
    if r.chance(0.4) {
        lock(sc, &rig, 90 + r.below(200), r);
    }
    if r.chance(0.3) {
        let t = 130 + r.below(150);
        sc.events.push(rig.give(t, AIR));
        sc.events.push(rig.give(t + 25, r.pick(&PAIRS).1));
    }
    if r.chance(0.5) {
        let t = 200 + r.below(160);
        sc.events.push(if r.chance(0.5) {
            act(
                t,
                depot,
                Action::Capability {
                    side: None,
                    slot: 1,
                    insert: None,
                    extract: Some(64),
                    simulate: false,
                },
            )
        } else {
            Action::Item {
                input: Input::Use {
                    stack: None,
                    face: Face::Up,
                    yaw: 0.,
                    sneaking: false,
                },
            }
            .into_change(t, depot)
        });
        sc.checkpoints.extend(t..t + 4);
    }
}

impl Action {
    fn into_change(self, tick: u32, pos: Pos) -> Change {
        act(tick, pos, self)
    }
}

/// A deployer facing down over a belt: the same recipes on stacks riding under it.
fn over_belt(sc: &mut Scenario, r: &mut Rng, rpm: i32) {
    let length = 4 + r.below(3) as i32;
    let speed = if r.chance(0.5) { 32 } else { -32 };
    sc.blocks.extend([
        placed([0, 8, 0], "create:shaft[axis=x,waterlogged=false]", None),
        placed(
            [0, 8, length - 1],
            "create:shaft[axis=x,waterlogged=false]",
            None,
        ),
        placed(
            [-1, 8, 0],
            "create:creative_motor[facing=east]",
            Some(speed),
        ),
    ]);
    for (tick, z) in [(8, 0), (14, length - 1)] {
        sc.events.push(use_on(
            tick,
            [0, 8, z],
            "create:belt_connector",
            Face::Up,
            [0.5, 1., 0.5],
        ));
    }
    let movement =
        create_engine::belt::movement_facing(speed as f32, create_engine::kinetics::Dir::South);
    let (end, start) = if movement == create_engine::kinetics::Dir::South {
        ([0, 8, length], 0)
    } else {
        ([0, 8, -1], length - 1)
    };
    sc.blocks.push(placed(end, "minecraft:stone", None));
    let rig = Rig::new([0, 10, 1], Face::Down, false);
    rig.build(sc, rpm);
    let pair = *r.pick(&PAIRS);
    sc.events.push(rig.give(20, pair.1));
    stacks(sc, r, [0, 8, start], pair, 40);
    if r.chance(0.4) {
        let t = 90 + r.below(200);
        sc.events.push(rig.wrench(t));
    }
    if r.chance(0.3) {
        lock(sc, &rig, 120 + r.below(180), r);
    }
}

/// A deployer holding a block item, on every facing: it places the block two ahead, waits while
/// its own block stands there, and never places against a wall (`BlockItemMixin`).
fn placing(sc: &mut Scenario, r: &mut Rng, rpm: i32) {
    let facing = *r.pick(&[
        Face::Down,
        Face::Up,
        Face::North,
        Face::South,
        Face::West,
        Face::East,
    ]);
    let rig = Rig::new([0, 8, 0], facing, r.chance(0.5));
    rig.build(sc, rpm);
    let (item, block) = *r.pick(&[
        ("minecraft:oak_planks", "minecraft:oak_planks"),
        ("minecraft:cobblestone", "minecraft:cobblestone"),
        ("minecraft:dirt", "minecraft:dirt"),
        ("create:andesite_casing", "create:andesite_casing"),
        ("minecraft:honeycomb_block", "minecraft:honeycomb_block"),
    ]);
    let far = rig.ahead(2);
    if [far[0].abs(), far[1], far[2].abs()].iter().any(|v| *v > 22) {
        return;
    }
    let pattern = r.below(4);
    match pattern {
        1 => {
            sc.blocks.push(placed(far, block, None));
        }
        2 => sc.blocks.push(placed(far, "minecraft:stone", None)),
        3 => sc
            .blocks
            .push(placed(rig.ahead(1), "minecraft:stone", None)),
        _ => {}
    }
    sc.events.push(rig.give(12, item));
    let change = 110 + r.below(80);
    match pattern {
        1 | 2 => {
            sc.events.push(Change::set(change, far, AIR));
        }
        _ => {
            sc.events.push(Change::set(change, far, "minecraft:stone"));
            if r.chance(0.5) {
                sc.events
                    .push(Change::set(change + 60 + r.below(60), far, AIR));
            }
        }
    }
    sc.checkpoints.extend(change..change + 4);
}

/// A punching deployer on a block it reaches: with an empty hand nothing gives, with a stick
/// little, with a pickaxe the block goes and its drops are the fake player's.
fn punching(sc: &mut Scenario, r: &mut Rng, rpm: i32) {
    let facing = *r.pick(&[Face::East, Face::North, Face::Down, Face::Up]);
    let rig = Rig::new([0, 8, 0], facing, true);
    rig.build(sc, rpm);
    let (held, target) = *r.pick(&[
        ("minecraft:air", "minecraft:stone"),
        ("minecraft:stick", "minecraft:stone"),
        ("minecraft:stick", "minecraft:cobblestone"),
        ("minecraft:stick", "minecraft:dirt"),
        ("minecraft:oak_planks", "minecraft:oak_planks"),
        ("minecraft:wooden_pickaxe", "minecraft:stone"),
        ("minecraft:iron_pickaxe", "minecraft:cobblestone"),
        ("minecraft:iron_pickaxe", "create:depot[waterlogged=false]"),
        ("minecraft:iron_axe", "minecraft:oak_planks"),
        (
            "minecraft:iron_pickaxe",
            "minecraft:grass_block[snowy=false]",
        ),
        ("minecraft:iron_pickaxe", "minecraft:bedrock"),
    ]);
    sc.blocks.push(placed(rig.ahead(2), target, None));
    sc.events.push(rig.wrench(8));
    if held != AIR {
        sc.events.push(rig.give(12, held));
    }
    let change = 120 + r.below(100);
    if target != "minecraft:bedrock" {
        let next = *r.pick(&[AIR, "minecraft:dirt", "minecraft:stone", target]);
        sc.events.push(rig.set_ahead(change, 2, next));
    }
    sc.checkpoints.extend(change..change + 4);
}

/// A deployer's use of an axe, a shovel, a hoe, an alloy and a wrench on the block it reaches.
fn tools(sc: &mut Scenario, r: &mut Rng, rpm: i32) {
    let rig = Rig::new([0, 10, 0], Face::Down, r.chance(0.5));
    rig.build(sc, rpm);
    // The axe strips the log, the shovel paths the grass, the hoe tills, an alloy or an ingot
    // makes a casing; a pickaxe on stone and an ingot on dirt are inert.
    let (held, target) = *r.pick(&[
        ("minecraft:iron_axe", "minecraft:oak_log[axis=y]"),
        ("minecraft:wooden_axe", "minecraft:oak_log[axis=y]"),
        (
            "minecraft:iron_shovel",
            "minecraft:grass_block[snowy=false]",
        ),
        ("minecraft:iron_hoe", "minecraft:dirt"),
        ("minecraft:iron_hoe", "minecraft:grass_block[snowy=false]"),
        (
            "create:andesite_alloy",
            "minecraft:stripped_oak_log[axis=y]",
        ),
        ("create:brass_ingot", "minecraft:stripped_oak_log[axis=y]"),
        (
            "minecraft:copper_ingot",
            "minecraft:stripped_oak_log[axis=y]",
        ),
        ("minecraft:iron_pickaxe", "minecraft:stone"),
        ("create:brass_ingot", "minecraft:dirt"),
    ]);
    sc.blocks.push(placed(rig.ahead(2), target, None));
    sc.events.push(rig.give(12, held));
    // A second use of the same tool on what the block became.
    if r.chance(0.5) {
        sc.events.push(rig.set_ahead(150 + r.below(60), 2, target));
    }
    if r.chance(0.4) {
        // Another tool on another block.
        let t = 220 + r.below(120);
        let (next, block) = *r.pick(&[
            ("minecraft:iron_axe", "minecraft:oak_log[axis=y]"),
            ("minecraft:iron_shovel", "minecraft:dirt"),
            ("minecraft:iron_hoe", "minecraft:dirt"),
            (
                "create:andesite_alloy",
                "minecraft:stripped_oak_log[axis=y]",
            ),
        ]);
        sc.events.push(rig.give(t, next));
        sc.events.push(rig.set_ahead(t, 2, block));
    }
}

/// Two punching deployers two to four blocks apart bump fists; one that uses, or stands still,
/// does not.
fn fist_bump(sc: &mut Scenario, r: &mut Rng, rpm: i32) {
    let apart = 2 + r.below(3) as i32;
    let a = Rig::new([0, 8, 0], Face::East, true);
    let b = Rig::new([apart, 8, 0], Face::West, true);
    a.build(sc, rpm);
    let still = r.chance(0.3);
    sc.blocks.push(placed(
        b.motor,
        b.motor_state,
        Some(if still { 0 } else { rpm }),
    ));
    sc.blocks
        .push(placed(b.at, &deployer_state(Face::West, true), None));
    sc.events.push(a.wrench(6));
    if r.chance(0.7) {
        sc.events.push(b.wrench(8));
    }
    if r.chance(0.3) {
        let t = 150 + r.below(150);
        sc.events.push(a.wrench(t));
    }
}

/// A deployer's filter: one naming the tool lets it work, one naming another item sends the
/// tool to the overflow, where it waits.
fn filtered(sc: &mut Scenario, r: &mut Rng, rpm: i32) {
    over_depot(sc, r, rpm, true);
}

/// The hand taken back and another given, in the middle of a cycle.
fn swapping(sc: &mut Scenario, r: &mut Rng, rpm: i32) {
    let depot = [0, 8, 0];
    let rig = Rig::new([0, 10, 0], Face::Down, false);
    sc.blocks.push(placed(depot, DEPOT, None));
    rig.build(sc, rpm);
    sc.events.push(rig.give(12, "minecraft:oak_planks"));
    sc.events.push(rig.give(14 + r.below(40), AIR));
    let second = *r.pick(&PAIRS);
    let at = 60 + r.below(30);
    sc.events.push(rig.give(at, second.1));
    sc.checkpoints.extend(at - 1..at + 4);
    stacks(sc, r, depot, second, at + 10);
    if r.chance(0.5) {
        let t = 230 + r.below(140);
        // A click with another item swaps the stacks.
        sc.events.push(rig.give(t, "minecraft:oak_planks"));
        sc.events.push(rig.give(t + 30, second.1));
    }
}

/// A deployer taken while empty and put back, its motor stopped and turned again, and its
/// depot taken under a stack it holds.
fn removed(sc: &mut Scenario, r: &mut Rng, rpm: i32) {
    let depot = [0, 8, 0];
    let rig = Rig::new([0, 10, 0], Face::Down, false);
    sc.blocks.push(placed(depot, DEPOT, None));
    rig.build(sc, rpm);
    sc.events.push(Change::set(20, rig.at, AIR));
    sc.events
        .push(Change::set(26, rig.at, &deployer_state(Face::Down, false)));
    sc.checkpoints.extend(20..30);
    let pair = *r.pick(&PAIRS);
    sc.events.push(rig.give(40, pair.1));
    stacks(sc, r, depot, pair, 60);
    let stop = 150 + r.below(80);
    sc.events.push(Change {
        tick: stop,
        pos: rig.motor,
        action: None,
        state: Some(rig.motor_state.into()),
        motor: Some(0),
        turn: None,
    });
    sc.events.push(Change {
        tick: stop + 40 + r.below(60),
        pos: rig.motor,
        action: None,
        state: Some(rig.motor_state.into()),
        motor: Some(if r.chance(0.5) { rpm } else { -rpm }),
        turn: None,
    });
    let gone = 300 + r.below(100);
    sc.events.push(Change::set(gone, depot, AIR));
    sc.events.push(Change::set(gone + 30, depot, DEPOT));
}

pub const VARIANTS: u64 = 9;

/// The histories of the `deployers` family, picked by `index`: one seed in three never
/// reloads, the others always do.
pub fn generate(seed: u64, index: u64) -> Scenario {
    let mut r = Rng::new(seed);
    let rpm = *r.pick(&[16, 32, 64, -32, 128, 256]);
    let mut sc = base(seed);
    match (index / 3) % VARIANTS {
        1 => over_belt(&mut sc, &mut r, rpm),
        2 => placing(&mut sc, &mut r, rpm),
        3 => punching(&mut sc, &mut r, rpm),
        4 => tools(&mut sc, &mut r, rpm),
        5 => fist_bump(&mut sc, &mut r, rpm),
        6 => filtered(&mut sc, &mut r, rpm),
        7 => swapping(&mut sc, &mut r, rpm),
        8 => removed(&mut sc, &mut r, rpm),
        _ => over_depot(&mut sc, &mut r, rpm, false),
    }
    sc.events.sort_by_key(|e| e.tick);
    // The first strokes tick by tick, then every fourth tick to the end, which waits for the
    // latest input to settle when one is picked late.
    let last = sc.events.iter().map(|e| e.tick).max().unwrap_or(0);
    sc.ticks = sc.ticks.max(last + crate::generate::SETTLE);
    sc.checkpoints.extend(10..=80);
    sc.checkpoints.extend((84..=sc.ticks).step_by(4));
    sc.checkpoints.sort_unstable();
    sc.checkpoints.dedup();
    sc.checkpoints.retain(|t| *t <= sc.ticks);
    sc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_history_is_valid_and_runs() {
        let mut variants = std::collections::BTreeSet::new();
        let mut reloaded = 0;
        for k in 0..9 {
            for residue in [64, 125, 253] {
                let seed = residue + 256 * k;
                let sc = crate::generate::generate(seed);
                crate::catalogue::scenario(&sc);
                crate::transport::validate_scenario(&sc);
                variants.insert(crate::generate::index(seed) / 3);
                reloaded += usize::from(
                    sc.events
                        .iter()
                        .any(|e| matches!(e.action, Some(Action::Reload))),
                );
                assert!(
                    sc.events
                        .iter()
                        .any(|e| matches!(e.action, Some(Action::Use { .. })))
                );
                assert!(sc.checkpoints.iter().all(|t| *t <= sc.ticks));
                let snapshot = crate::engine::run(&sc);
                let deployers = snapshot
                    .blocks
                    .values()
                    .filter(|b| b.deployer.is_some())
                    .count();
                assert!(deployers >= 1, "seed {seed}");
                let (_, rank) = (seed / 256, seed % 256);
                assert_eq!(
                    rank == 64,
                    !sc.events
                        .iter()
                        .any(|e| matches!(e.action, Some(Action::Reload))),
                    "seed {seed}"
                );
            }
        }
        assert_eq!(variants, (0..VARIANTS).collect());
        assert!(reloaded >= 6);
    }

    #[test]
    fn every_seed_of_the_three_lanes_is_a_valid_history() {
        // Beyond the 27 variant seeds: the later picks reach event times past `TICKS`.
        for residue in [64, 125, 253] {
            for k in 0..64 {
                let seed = residue + 256 * k;
                let sc = crate::generate::generate(seed);
                crate::catalogue::scenario(&sc);
                crate::transport::validate_scenario(&sc);
                assert!(sc.ticks >= TICKS, "seed {seed}");
            }
        }
    }
    #[test]
    fn the_contract_names_its_own_states() {
        let c = contract();
        for i in &c.items {
            if let Some(b) = i.block.as_ref().filter(|b| b.starts_with("minecraft:")) {
                assert!(c.states.contains(b), "{b}");
            }
            for to in i.turns.values() {
                assert!(
                    c.states
                        .iter()
                        .any(|s| crate::scenario::parse_state(s).0 == *to),
                    "{to}"
                );
            }
        }
        for b in &c.blocks {
            assert!(c.states.contains(&b.state), "{}", b.state);
        }
        for d in &c.dig {
            assert!(c.items.iter().any(|i| i.id == d.held), "{}", d.held);
            assert!(c.blocks.iter().any(|b| b.state == d.block), "{}", d.block);
        }
        assert!(id("minecraft:dirt").is_some());
        assert!(id("minecraft:dirt[grass=true]").is_none());
        assert!(known_block("minecraft:stone"));
        assert!(known_block("create:deployer"));
        assert!(!known_block("minecraft:gravel"));
    }
}
