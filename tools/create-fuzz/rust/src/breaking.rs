//! Stationary block breaking: the mechanical drill and the sideways saw
//! (`BlockBreakingKineticBlockEntity`; see docs/processing-scope.md). The vanilla blocks they
//! work on (`breaking.json`, checked by the server against the loaded game), what they let the
//! comparison see, and the scenario families.
use crate::{
    actions::{Action, Face},
    generate::Rng,
    scenario::{Change, Placed, Pos, Scenario, parse_state},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// A breaker's state as Create saves it (`BlockBreakingKineticBlockEntity.write`): `destroyProgress`
/// and `ticksUntilNextProgress`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Breaking {
    pub progress: i32,
    pub next_tick: i32,
}

#[derive(Deserialize)]
pub struct Manifest {
    pub blocks: Vec<Block>,
    pub tags: BTreeMap<String, Vec<String>>,
    /// `TreeCutter.isVerticalPlant`'s blocks.
    pub vertical: Vec<String>,
}

/// A vanilla block: its hardness (`BlockState.getDestroySpeed`), what `Block.getDrops` gives a
/// breaker (no tool, no player): fixed stacks, or the items chance decides (`rolled`), and what
/// `TreeCutter` and the game do with it.
#[derive(Deserialize)]
pub struct Block {
    pub id: String,
    #[serde(default)]
    pub properties: BTreeMap<String, Vec<String>>,
    pub hardness: f32,
    #[serde(default)]
    pub drops: Vec<Stack>,
    #[serde(default)]
    pub rolled: Vec<String>,
    #[serde(default)]
    pub falls: bool,
    #[serde(default)]
    pub leaf: bool,
    /// The blocks it stands on (`canSurvive`); empty when it needs none.
    #[serde(default)]
    pub stands_on: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Stack {
    pub item: String,
    pub count: u8,
}

/// What a broken block drops as the host's `destroy_block(pos, true, ..)` rolls it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Loot {
    Fixed(Vec<(String, u8)>),
    /// Chance decides whether and how many: the comparison leaves these items out.
    Rolled(Vec<String>),
}

pub fn contract() -> &'static Manifest {
    static M: std::sync::OnceLock<Manifest> = std::sync::OnceLock::new();
    M.get_or_init(|| serde_json::from_str(include_str!("breaking.json")).expect("breaking.json"))
}

/// The catalogue's entry for the block `id`.
pub fn block(id: &str) -> Option<&'static Block> {
    contract().blocks.iter().find(|b| b.id == id)
}

/// Whether `state` is one of the catalogue's blocks.
pub fn known(state: &str) -> bool {
    block(&parse_state(state).0).is_some()
}

pub fn validate_state(id: &str, props: &[(String, String)]) -> Option<Result<(), String>> {
    let entry = block(id)?;
    Some(
        if props
            .iter()
            .all(|(k, v)| entry.properties.get(k).is_some_and(|vs| vs.contains(v)))
        {
            Ok(())
        } else {
            Err(format!("unsupported breaking state {id}: {props:?}"))
        },
    )
}

/// The state with the properties the scenario left out at the block's default, as the server's
/// `setblock` fills them.
pub fn full_state(state: &str) -> Option<String> {
    let (id, mut props) = parse_state(state);
    let entry = block(&id)?;
    for (key, values) in &entry.properties {
        if !props.iter().any(|(k, _)| k == key) {
            props.push((key.clone(), values[0].clone()));
        }
    }
    Some(crate::scenario::format_state(&id, &props))
}

/// A property of a catalogue state, the default where the state omits it.
pub fn property<'a>(state: &'a str, key: &str) -> Option<std::borrow::Cow<'a, str>> {
    let (id, props) = parse_state(state);
    if let Some((_, v)) = props.iter().find(|(k, _)| k == key) {
        return Some(std::borrow::Cow::Owned(v.clone()));
    }
    Some(std::borrow::Cow::Borrowed(
        block(&id)?.properties.get(key)?.first()?.as_str(),
    ))
}

/// The same state with `key` at `value`.
pub fn with(state: &str, key: &str, value: &str) -> String {
    crate::scenario::with_prop(state, key, value)
}

pub fn tagged(state: &str, tag: &str) -> bool {
    let id = parse_state(state).0;
    contract()
        .tags
        .get(tag)
        .is_some_and(|ids| ids.contains(&id))
}

/// The hardness the host reports for `state`: the catalogue's, or the port's declaration for a
/// Create block.
pub fn hardness(state: &str) -> Option<f32> {
    block(&parse_state(state).0)
        .map(|b| b.hardness)
        .or_else(|| crate::loot::hardness(state))
}

/// The loot of `state` as a breaker drops it; None when neither side declares it.
pub fn loot(state: &str) -> Option<Loot> {
    if let Some(b) = block(&parse_state(state).0) {
        return Some(if b.rolled.is_empty() {
            Loot::Fixed(b.drops.iter().map(|s| (s.item.clone(), s.count)).collect())
        } else {
            Loot::Rolled(b.rolled.clone())
        });
    }
    crate::loot::drops(state).map(Loot::Fixed)
}

/// The items chance decides in two snapshots: those of every catalogue block with rolled loot
/// that either shows anywhere, checkpoints included.
pub fn chance_in(a: &crate::scenario::Snapshot, b: &crate::scenario::Snapshot) -> BTreeSet<String> {
    fn see(s: &crate::scenario::Snapshot, out: &mut BTreeSet<String>) {
        for snap in s.blocks.values() {
            if let Some(b) = block(&parse_state(&snap.state).0).filter(|b| !b.rolled.is_empty()) {
                out.extend(b.rolled.iter().cloned());
            }
        }
        for c in s.checkpoints.values() {
            see(c, out);
        }
    }
    let mut out = BTreeSet::new();
    see(a, &mut out);
    see(b, &mut out);
    out
}

/// `s` and its checkpoints without the drops of `items`.
pub fn without_drops(s: &mut crate::scenario::Snapshot, items: &BTreeSet<String>) {
    if let Some(drops) = s.drops.as_mut() {
        drops.retain(|d| !items.contains(&d.stack.item));
    }
    for c in s.checkpoints.values_mut() {
        without_drops(c, items);
    }
}

/// Equal breaker states.
pub fn equal(a: Option<&Breaking>, b: Option<&Breaking>) -> bool {
    a == b
}

fn scenario(seed: u64, ticks: u32) -> Scenario {
    Scenario {
        version: 3,
        seed,
        ticks,
        day_time: 0,
        checkpoints: vec![],
        blocks: vec![],
        events: vec![],
    }
}

fn put(sc: &mut Scenario, pos: Pos, state: impl Into<String>) {
    sc.blocks.push(Placed {
        pos,
        state: state.into(),
        motor: None,
    });
}

fn set(tick: u32, pos: Pos, state: impl Into<String>) -> Change {
    Change::set(tick, pos, &state.into())
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

/// A creative motor turning at `rpm`, its shaft out of its `facing` face.
fn motor(sc: &mut Scenario, pos: Pos, facing: &str, rpm: i32) {
    sc.blocks.push(Placed {
        pos,
        state: format!("create:creative_motor[facing={facing}]"),
        motor: Some(rpm),
    });
}

/// The motor back in place at `tick`; its speed is set the tick after.
fn motor_event(tick: u32, pos: Pos, facing: &str, rpm: i32) -> Change {
    Change {
        tick,
        pos,
        action: None,
        state: Some(format!("create:creative_motor[facing={facing}]")),
        motor: Some(rpm),
        turn: None,
    }
}

/// A drill or a saw facing `facing` at `pos`.
fn breaker(sc: &mut Scenario, pos: Pos, facing: &str, saw: bool, along_first: bool) {
    let state = if saw {
        format!(
            "create:mechanical_saw[axis_along_first={along_first},facing={facing},flipped=false]"
        )
    } else {
        format!("create:mechanical_drill[facing={facing},waterlogged=false]")
    };
    put(sc, pos, state);
}

fn dir(name: &str) -> create_engine::kinetics::Dir {
    create_engine::kinetics::Dir::ALL
        .into_iter()
        .find(|d| format!("{d:?}").eq_ignore_ascii_case(name))
        .expect("direction")
}

/// A quarter turn clockwise from a horizontal `facing`, seen from above: beside a breaker and
/// the block it works on, never on either.
fn beside(facing: &str) -> Pos {
    match facing {
        "east" => [0, 0, 1],
        "south" => [-1, 0, 0],
        "west" => [0, 0, -1],
        _ => [1, 0, 0],
    }
}

/// The block in front of a breaker facing `facing`, and the one behind it.
pub fn around(block: Pos, facing: &str) -> (Pos, Pos) {
    let o = dir(facing).offset();
    (
        [block[0] + o[0], block[1] + o[1], block[2] + o[2]],
        [block[0] - o[0], block[1] - o[1], block[2] - o[2]],
    )
}

/// The speeds the families turn at, in rpm.
pub const SPEEDS: [i32; 6] = [8, 16, 64, 128, 256, -48];
const FACINGS: [&str; 6] = ["east", "north", "west", "south", "up", "down"];
const HORIZONTAL: [&str; 4] = ["east", "north", "west", "south"];

/// Checkpoints every tick to `dense`, then every other tick, and four ticks around each event.
fn checkpoints(sc: &mut Scenario, dense: u32) {
    let events: Vec<u32> = sc.events.iter().map(|e| e.tick).collect();
    sc.checkpoints = (0..=sc.ticks)
        .filter(|t| {
            *t <= dense
                || t.is_multiple_of(2)
                || events
                    .iter()
                    .any(|e| e.saturating_sub(3) <= *t && *t <= e + 4)
        })
        .collect();
}

const KINDS: [&str; 9] = [
    "minecraft:bedrock",
    "minecraft:tnt",
    "minecraft:glass",
    "minecraft:coal_ore",
    "minecraft:gravel",
    "minecraft:cobblestone",
    "minecraft:clay",
    "minecraft:oak_log",
    "minecraft:water[level=0]",
];

/// `breaking/drill`: a drill on a motor against the block in front of it. By its index one of
/// twelve histories on six facings at six speeds. The target stands there from the start; or it
/// is swapped for another breakable block, an unbreakable one and back; or taken away while it
/// cracks and put back; or the drill is removed while it cracks; or its motor is removed while
/// it cracks and replaced; or the world is saved and reloaded while it cracks and after a break;
/// or a block arrives in front of an idle drill; or a sand column refills the block it breaks;
/// or `doTileDrops` is turned off; or a second drill works the same block; or the target is of
/// another kind; or the drill is wrenched while it cracks.
pub fn drill(seed: u64, index: u64) -> Scenario {
    const VARIANTS: u64 = 12;
    let variant = index % VARIANTS;
    let n = index / VARIANTS;
    let rpm = SPEEDS[(n % 6) as usize];
    let kind = KINDS[(n % 9) as usize];
    let facing = FACINGS[(n / 6 % 6) as usize];
    let facing = if matches!(variant, 7 | 9) && !HORIZONTAL.contains(&facing) {
        HORIZONTAL[(n / 6 % 4) as usize]
    } else {
        facing
    };
    let level = 8 + (n / 18 % 4) as i32 * 3;
    let mut r = Rng::new(seed);
    let d = [1, level, 0];
    let (t, m) = around(d, facing);
    let slow = matches!(variant, 3 | 4 | 5 | 11);
    let target = match variant {
        10 => kind,
        _ if slow => "minecraft:obsidian",
        _ => "minecraft:stone",
    };
    let mut sc = scenario(seed, 0);
    // What keeps a falling target from falling off the world (a block above the drill rests
    // on it).
    let support = [t[0], t[1] - 1, t[2]];
    if (variant == 7 || (variant == 10 && kind == "minecraft:gravel")) && support != d {
        put(&mut sc, support, "minecraft:stone");
    }
    motor(&mut sc, m, facing, rpm);
    breaker(&mut sc, d, facing, false, false);
    match variant {
        6 => {}
        7 => {
            let column = 1 + r.below(3) as i32;
            for h in 0..=column {
                put(&mut sc, [t[0], t[1] + h, t[2]], "minecraft:sand");
            }
            put(&mut sc, [t[0], t[1] + column + 1, t[2]], "minecraft:stone");
        }
        10 if kind == "minecraft:water[level=0]" => {
            // A source the drill cannot break, held by stone on its five sides away from the
            // drill; the walls stand before the water comes.
            for side in [
                [0, 1, 0],
                [0, -1, 0],
                [1, 0, 0],
                [-1, 0, 0],
                [0, 0, 1],
                [0, 0, -1],
            ] {
                let at = [t[0] + side[0], t[1] + side[1], t[2] + side[2]];
                if at != d && at != m {
                    put(&mut sc, at, "minecraft:stone");
                }
            }
            put(&mut sc, t, kind);
        }
        _ => put(&mut sc, t, target),
    }
    if variant == 9 {
        // A second drill on the far side of the target, facing it.
        let (d2, _) = around(t, facing);
        let o = dir(facing).offset();
        let m2 = [d2[0] + o[0], d2[1] + o[1], d2[2] + o[2]];
        let opposite = format!("{:?}", dir(facing).opposite()).to_lowercase();
        motor(&mut sc, m2, &opposite, rpm);
        breaker(&mut sc, d2, &opposite, false, false);
    }
    let placed = sc.blocks.len() as u32;
    let mut settle = 70u32;
    let mut event = |sc: &mut Scenario, e: Change| {
        settle = settle.max(e.tick + 70);
        sc.events.push(e);
    };
    let at = placed + 18;
    match variant {
        1 => {
            event(&mut sc, set(at, t, "minecraft:dirt"));
            event(&mut sc, set(placed + 45, t, "minecraft:bedrock"));
            event(&mut sc, set(placed + 80, t, target));
        }
        2 => {
            event(&mut sc, set(at, t, "minecraft:air"));
            event(&mut sc, set(placed + 60, t, target));
        }
        3 => event(&mut sc, set(at, d, "minecraft:air")),
        4 => {
            event(&mut sc, set(at, m, "minecraft:air"));
            event(&mut sc, motor_event(placed + 70, m, facing, rpm));
        }
        5 => {
            event(&mut sc, act(at, d, Action::Reload));
            event(&mut sc, act(placed + 90, d, Action::Reload));
        }
        6 => event(&mut sc, set(placed + 40, t, "minecraft:stone")),
        8 => event(
            &mut sc,
            act(
                placed + 5,
                d,
                Action::GameRule {
                    do_tile_drops: false,
                },
            ),
        ),
        11 => {
            let (face, hit) = if HORIZONTAL.contains(&facing) {
                (Face::Up, [0.5, 1.0, 0.5])
            } else {
                (Face::South, [0.5, 0.5, 1.0])
            };
            event(
                &mut sc,
                act(
                    at,
                    d,
                    Action::Use {
                        item: "create:wrench".into(),
                        face,
                        hit,
                        yaw: 0.0,
                        pitch: 0.0,
                        sneaking: false,
                    },
                ),
            );
        }
        _ => {}
    }
    sc.ticks = settle + placed;
    checkpoints(&mut sc, placed + 14);
    sc
}

const WOODS: [(&str, &str); 3] = [
    ("minecraft:oak_log", "minecraft:oak_leaves"),
    ("minecraft:spruce_log", "minecraft:spruce_leaves"),
    ("minecraft:birch_log", "minecraft:birch_leaves"),
];

fn leaf(leaf: &str, distance: i32) -> String {
    format!("{leaf}[distance={distance},persistent=false,waterlogged=false]")
}

/// A tree of `height` logs over the cut at `t`, its top ringed with two layers of leaves; the
/// leaves from the trunk outwards, or from the rim inwards.
fn tree(sc: &mut Scenario, t: Pos, height: i32, wood: usize, centre_out: bool) {
    let (log, leaves) = WOODS[wood];
    column(sc, t, 1..=height, &format!("{log}[axis=y]"));
    let top = t[1] + height;
    let mut rim = Vec::new();
    for dy in 0..2 {
        for dx in -1..=1 {
            for dz in -1..=1 {
                if dx == 0 && dz == 0 && dy == 0 {
                    continue;
                }
                rim.push([t[0] + dx, top + dy, t[2] + dz]);
            }
        }
    }
    if !centre_out {
        rim.reverse();
    }
    for p in rim {
        put(sc, p, leaf(leaves, 1));
    }
}

/// A column of `state` over `t`, from `ys` blocks above it.
fn column(sc: &mut Scenario, t: Pos, ys: impl Iterator<Item = i32>, state: &str) {
    for y in ys {
        put(sc, [t[0], t[1] + y, t[2]], state);
    }
}

/// `breaking/saw`: a sideways saw on a motor against what stands in front of it. By its index
/// one of fifteen histories on the four horizontal facings at six speeds: a log alone; a tree
/// whose trunk it cuts through; a cut a second trunk holds up; a trunk with a branch; mangrove
/// roots; a mushroom stem among mushroom blocks and a shroomlight; a bamboo stalk; a pumpkin or
/// a melon; what a saw cannot cut; a crimson stem among wart blocks; a tree with a bee nest and
/// moss carpet on it; leaves alone; the saw removed while it cuts; its motor removed while it
/// cuts and replaced; `doTileDrops` turned off.
pub fn saw(seed: u64, index: u64) -> Scenario {
    const VARIANTS: u64 = 15;
    let variant = index % VARIANTS;
    let n = index / VARIANTS;
    let rpm = SPEEDS[(n % 6) as usize];
    let facing = HORIZONTAL[(n / 6 % 4) as usize];
    let along_first = (n / 24).is_multiple_of(2);
    let level = 8 + (n / 2 % 4) as i32 * 3;
    let wood = (n / 4 % 3) as usize;
    let height = 1 + (n / 12 % 4) as i32;
    let centre_out = (n / 48).is_multiple_of(2);
    let s = [1, level, 0];
    let (t, m) = around(s, facing);
    let side = beside(facing);
    let (log, leaves) = WOODS[wood];
    let log = format!("{log}[axis=y]");
    let mut sc = scenario(seed, 0);
    // Both stand on dirt or grass (sand would fall off the empty world here).
    let ground = match variant {
        6 => ["minecraft:dirt", "minecraft:grass_block"][(n % 2) as usize],
        8 => ["minecraft:dirt", "minecraft:grass_block"][((n / 2) % 2) as usize],
        _ => "",
    };
    if !ground.is_empty() {
        put(&mut sc, [t[0], t[1] - 1, t[2]], ground);
    }
    motor(&mut sc, m, facing, rpm);
    breaker(&mut sc, s, facing, true, along_first);
    match variant {
        0 => put(&mut sc, t, &log),
        1 => tree(&mut sc, t, height, wood, centre_out),
        2 => {
            // Two trunks joined above the cut: the other one holds the layer up.
            for h in 0..3 {
                put(&mut sc, [t[0], t[1] + h, t[2]], &log);
                put(&mut sc, [t[0] + side[0], t[1] + h, t[2] + side[2]], &log);
            }
        }
        3 => {
            column(&mut sc, t, 0..3, &log);
            put(&mut sc, [t[0], t[1] + 2, t[2] + 1], &log);
            for (dx, dy, dz) in [
                (0, 3, 0),
                (1, 2, 0),
                (-1, 2, 0),
                (0, 2, 2),
                (1, 3, 1),
                (0, 2, -1),
            ] {
                put(&mut sc, [t[0] + dx, t[1] + dy, t[2] + dz], leaf(leaves, 1));
            }
        }
        4 => {
            let height = 1 + (n % 3) as i32;
            column(&mut sc, t, 0..height, "minecraft:mangrove_roots");
            put(
                &mut sc,
                [t[0] + side[0], t[1], t[2] + side[2]],
                "minecraft:mangrove_roots",
            );
            put(&mut sc, [t[0], t[1] + height, t[2]], &log);
        }
        5 => {
            column(&mut sc, t, 0..3, "minecraft:mushroom_stem");
            for (dx, dy, dz, state) in [
                (1, 2, 0, "minecraft:red_mushroom_block"),
                (0, 3, 0, "minecraft:brown_mushroom_block"),
                (-1, 2, 0, "minecraft:red_mushroom_block"),
                (0, 2, 1, "minecraft:brown_mushroom_block"),
                (1, 3, 1, "minecraft:shroomlight"),
            ] {
                put(&mut sc, [t[0] + dx, t[1] + dy, t[2] + dz], state);
            }
        }
        6 => column(
            &mut sc,
            t,
            0..2 + (n % 3) as i32,
            "minecraft:bamboo[age=1,leaves=none,stage=0]",
        ),
        7 => {
            let crop = if n.is_multiple_of(2) {
                "minecraft:pumpkin"
            } else {
                "minecraft:melon"
            };
            put(&mut sc, t, crop);
        }
        8 => {
            let state = match n % 3 {
                0 => "minecraft:stone",
                1 => "minecraft:oak_planks",
                _ => "minecraft:oak_sapling[stage=0]",
            };
            put(&mut sc, t, state);
        }
        9 => {
            column(&mut sc, t, 0..3, "minecraft:crimson_stem[axis=y]");
            for (dx, dy, dz, state) in [
                (1, 2, 0, "minecraft:nether_wart_block"),
                (0, 3, 0, "minecraft:warped_wart_block"),
                (0, 2, 1, "minecraft:nether_wart_block"),
                (2, 2, 0, "minecraft:nether_wart_block"),
            ] {
                put(&mut sc, [t[0] + dx, t[1] + dy, t[2] + dz], state);
            }
        }
        10 => {
            tree(&mut sc, t, 3, wood, true);
            put(
                &mut sc,
                [t[0], t[1] + 2, t[2] + 1],
                "minecraft:bee_nest[facing=north,honey_level=0]",
            );
            put(&mut sc, [t[0], t[1] + 2, t[2] - 1], "minecraft:moss_carpet");
            put(
                &mut sc,
                [t[0] + 1, t[1] + 1, t[2]],
                "minecraft:bee_nest[facing=west,honey_level=3]",
            );
        }
        11 => {
            for dy in 0..2 {
                for dx in -1i32..=1 {
                    for dz in -1i32..=1 {
                        if dx == 0 && dz == 0 && dy == 0 {
                            continue;
                        }
                        let at = [t[0] + dx, t[1] + dy, t[2] + dz];
                        if at == s || at == m {
                            continue;
                        }
                        let distance = 1 + (dx.abs() + dz.abs() + dy).min(2);
                        put(&mut sc, at, leaf(leaves, distance));
                    }
                }
            }
        }
        // The saw removed, its motor removed, the rule off: against a log.
        _ => put(&mut sc, t, &log),
    }
    let placed = sc.blocks.len() as u32;
    let mut settle = 70u32;
    let mut event = |sc: &mut Scenario, e: Change| {
        settle = settle.max(e.tick + 70);
        sc.events.push(e);
    };
    let at = placed + 6;
    match variant {
        12 => event(&mut sc, set(at, s, "minecraft:air")),
        13 => {
            event(&mut sc, set(at, m, "minecraft:air"));
            event(&mut sc, motor_event(placed + 50, m, facing, rpm));
        }
        14 => event(
            &mut sc,
            act(
                placed + 5,
                s,
                Action::GameRule {
                    do_tile_drops: false,
                },
            ),
        ),
        _ => {}
    }
    sc.ticks = settle + placed;
    checkpoints(&mut sc, placed + 14);
    sc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observation::BlockSnap;
    use crate::scenario::Snapshot;
    use crate::transport::{DroppedItem, Stack as TransportStack};

    const LEAF: &str = "minecraft:oak_leaves[distance=1,persistent=false,waterlogged=false]";
    const DEFAULT_LEAF: &str =
        "minecraft:oak_leaves[distance=7,persistent=false,waterlogged=false]";

    #[test]
    fn the_catalogue_is_consistent() {
        let c = contract();
        let ids: BTreeSet<_> = c.blocks.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(ids.len(), c.blocks.len());
        let rolled: BTreeSet<_> = c
            .blocks
            .iter()
            .flat_map(|b| b.rolled.iter().map(String::as_str))
            .collect();
        // The comparison leaves a rolled item out wherever a case drops it, so nothing a block
        // drops for sure may be one. The one overlap is the oak sapling, which oak leaves roll:
        // no breaker in the families breaks a sapling (a saw cannot; a drill is never set on one).
        for b in &c.blocks {
            assert!(b.rolled.is_empty() || b.drops.is_empty(), "{}", b.id);
            for d in &b.drops {
                assert!(
                    d.item == "minecraft:oak_sapling" || !rolled.contains(d.item.as_str()),
                    "{} {}",
                    b.id,
                    d.item
                );
            }
            assert!(
                c.vertical.contains(&b.id) == (b.id == "minecraft:bamboo"),
                "{}",
                b.id
            );
            assert!(
                b.falls == matches!(b.id.as_str(), "minecraft:sand" | "minecraft:gravel"),
                "{}",
                b.id
            );
            assert!(b.leaf == b.properties.contains_key("distance"), "{}", b.id);
            assert!(b.stands_on.iter().all(|g| ids.contains(g.as_str())));
            assert_eq!(hardness(&b.id), Some(b.hardness));
        }
        for (tag, members) in &c.tags {
            for id in members {
                assert!(ids.contains(id.as_str()), "{tag}: {id}");
            }
            for b in &c.blocks {
                assert_eq!(
                    members.contains(&b.id),
                    tagged(&b.id, tag),
                    "{tag}: {}",
                    b.id
                );
            }
        }
        assert_eq!(
            loot("minecraft:stone"),
            Some(Loot::Fixed(vec![("minecraft:cobblestone".to_owned(), 1)]))
        );
        assert_eq!(
            loot("minecraft:gravel"),
            Some(Loot::Rolled(vec![
                "minecraft:gravel".to_owned(),
                "minecraft:flint".to_owned()
            ]))
        );
        assert_eq!(loot("minecraft:bedrock"), Some(Loot::Fixed(vec![])));
        assert_eq!(
            loot("minecraft:oak_leaves[distance=3,persistent=false,waterlogged=false]"),
            loot("minecraft:oak_leaves")
        );
        assert!(loot("create:mechanical_saw").is_some());
        assert!(hardness("create:mechanical_drill[facing=east,waterlogged=false]").is_some());
        assert_eq!(
            full_state("minecraft:oak_leaves"),
            Some(DEFAULT_LEAF.to_owned())
        );
        assert_eq!(
            full_state("minecraft:bamboo[leaves=small]"),
            Some("minecraft:bamboo[age=0,leaves=small,stage=0]".to_owned())
        );
        assert_eq!(
            property("minecraft:oak_leaves", "persistent"),
            Some("false".into())
        );
        assert_eq!(
            property(
                "minecraft:oak_leaves[distance=4,persistent=false,waterlogged=false]",
                "distance"
            ),
            Some("4".into())
        );
        assert!(full_state("minecraft:unknown_block").is_none());
        assert!(full_state("minecraft:redstone_block").is_none());
        let far = [("distance".to_owned(), "8".to_owned())];
        assert!(
            validate_state("minecraft:oak_leaves", &far)
                .unwrap()
                .is_err()
        );
        assert!(validate_state("create:shaft", &[]).is_none());
        assert_eq!(
            with(LEAF, "distance", "3"),
            "minecraft:oak_leaves[distance=3,persistent=false,waterlogged=false]"
        );
        assert!(tagged("minecraft:oak_log[axis=y]", "minecraft:logs"));
        assert!(!tagged("minecraft:mushroom_stem", "minecraft:logs"));
    }

    #[test]
    fn rolled_items_are_left_to_chance_on_both_sides() {
        let mut java = Snapshot {
            drops: Some(vec![DroppedItem {
                tick: 9,
                stack: TransportStack {
                    item: "minecraft:stick".into(),
                    count: 2,
                },
                pos: [0.5; 3],
                motion: [0.0; 3],
                placed: None,
            }]),
            ..Snapshot::default()
        };
        java.blocks.insert(
            "0,8,0".into(),
            BlockSnap {
                state: LEAF.into(),
                ..Default::default()
            },
        );
        let empty = Snapshot {
            drops: Some(vec![]),
            ..Snapshot::default()
        };
        assert_eq!(
            chance_in(&java, &empty),
            [
                "minecraft:apple".to_owned(),
                "minecraft:oak_sapling".to_owned(),
                "minecraft:stick".to_owned()
            ]
            .into()
        );
        // One side showing the leaf is enough for both to leave its chance items out.
        let mut shown = empty.clone();
        shown.blocks.insert(
            "0,8,0".into(),
            BlockSnap {
                state: LEAF.into(),
                ..Default::default()
            },
        );
        assert_eq!(crate::compare::diff(&java, &shown).len(), 0);
        assert_eq!(crate::compare::diff(&shown, &java).len(), 0);
        // With no side showing the leaf, the stick is a difference.
        let mut hidden = java.clone();
        hidden.blocks.clear();
        let differences = crate::compare::diff(&hidden, &empty);
        assert_eq!(differences.len(), 1);
        assert!(differences[0].starts_with("drops:"));
    }

    fn facing_of(sc: &Scenario, block: &str) -> String {
        sc.blocks
            .iter()
            .find(|b| b.state.starts_with(block))
            .map(|b| {
                parse_state(&b.state)
                    .1
                    .into_iter()
                    .find(|(k, _)| k == "facing")
                    .map(|(_, v)| v)
                    .unwrap()
            })
            .unwrap()
    }

    #[test]
    fn every_history_speed_and_facing_is_dealt() {
        let mut seen = BTreeSet::new();
        for index in 0..432 {
            let sc = drill(7, index);
            let rpm = sc.blocks.iter().find_map(|b| b.motor).unwrap();
            let reloads = sc
                .events
                .iter()
                .any(|e| matches!(&e.action, Some(Action::Reload)));
            assert_eq!(reloads, index % 12 == 5, "index {index}");
            seen.insert((index % 12, rpm, facing_of(&sc, "create:mechanical_drill")));
        }
        for history in 0..12 {
            for rpm in SPEEDS {
                for facing in FACINGS {
                    if matches!(history, 7 | 9) && !HORIZONTAL.contains(&facing) {
                        continue;
                    }
                    assert!(
                        seen.contains(&(history, rpm, facing.to_owned())),
                        "history {history} at {rpm} facing {facing}"
                    );
                }
            }
        }
        let mut seen = BTreeSet::new();
        for index in 0..360 {
            let sc = saw(7, index);
            let rpm = sc.blocks.iter().find_map(|b| b.motor).unwrap();
            assert!(
                !sc.events
                    .iter()
                    .any(|e| matches!(&e.action, Some(Action::Reload)))
            );
            seen.insert((index % 15, rpm, facing_of(&sc, "create:mechanical_saw")));
        }
        for history in 0..15 {
            for rpm in SPEEDS {
                for facing in HORIZONTAL {
                    assert!(
                        seen.contains(&(history, rpm, facing.to_owned())),
                        "history {history} at {rpm} facing {facing}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_histories_are_bounded_and_settle() {
        for family in [drill as fn(u64, u64) -> Scenario, saw] {
            for index in 0..2160u64 {
                let sc = family(index * 7919 + 3, index);
                let mut places: Vec<Pos> = sc.blocks.iter().map(|b| b.pos).collect();
                places.sort();
                places.dedup();
                assert_eq!(places.len(), sc.blocks.len(), "index {index}");
                crate::catalogue::scenario(&sc);
                assert!(
                    sc.blocks.iter().all(|b| {
                        b.pos[0].abs() <= 23 && b.pos[2].abs() <= 23 && (2..=22).contains(&b.pos[1])
                    }),
                    "index {index}"
                );
                assert!(
                    sc.events.iter().all(|e| e.tick + 70 <= sc.ticks),
                    "index {index}"
                );
                assert!(sc.ticks >= 70 + sc.blocks.len() as u32);
            }
        }
    }
}
