//! Basins under a mechanical mixer or a mechanical press (`MechanicalMixerBlockEntity`,
//! `MechanicalPressBlockEntity` as a basin operator, `BasinBlockEntity`): what is put into
//! them, mixed or compacted, taken out through their spout, and the heat under them. The
//! recipe subset both sides run (`basin.json`, audited by the server at startup), and the
//! scenario families.
use crate::{
    actions::{Action, Face},
    generate::Rng,
    processing::ManifestOutput,
    scenario::{Change, Placed, Pos, Scenario},
    transport::{Input, Stack},
};
use create_engine::{basin, processing::Output, tank::Fluid};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub inputs: Inputs,
    pub mixing: Vec<ManifestRecipe>,
    pub shapeless: Vec<ManifestRecipe>,
    pub compacting: Vec<ManifestRecipe>,
    pub squares: Vec<Square>,
}

#[derive(Debug, Deserialize)]
pub struct Inputs {
    pub mixing: Vec<String>,
    pub compacting: Vec<String>,
}

/// A basin recipe as Create's data writes it: each ingredient one of the family's items it
/// takes, its fluid ingredients and results by fluid and amount, the heat it needs.
#[derive(Debug, Deserialize)]
pub struct ManifestRecipe {
    pub id: String,
    #[serde(default)]
    pub ingredients: Vec<Vec<String>>,
    #[serde(default)]
    pub fluids: Vec<(String, u32)>,
    #[serde(default)]
    pub results: Vec<ManifestOutput>,
    #[serde(default)]
    pub fluid_results: Vec<(String, u32)>,
    #[serde(default)]
    pub heat: String,
}

/// A square crafting recipe the press compacts (`canCompress`): `size` by `size` of one item.
#[derive(Debug, Deserialize)]
pub struct Square {
    pub id: String,
    pub size: usize,
    pub item: String,
    pub result: String,
    pub count: u8,
}

pub fn contract() -> &'static Manifest {
    static M: std::sync::OnceLock<Manifest> = std::sync::OnceLock::new();
    M.get_or_init(|| serde_json::from_str(include_str!("basin.json")).expect("basin.json"))
}

/// An item's stack limit as the game gives it: the transport contract's, 64 for a result the
/// families never offer.
pub fn limit(item: &str) -> u8 {
    crate::transport::contract()
        .items
        .iter()
        .find(|i| i.id == item)
        .map_or(64, |i| i.max_stack)
}

/// `getCraftingRemainingItem` of the families' items: what emptying a container leaves.
pub fn remainder(item: &str) -> Option<String> {
    crate::fluid_recipes::contents(item).and_then(|e| e.leftover)
}

/// The recipes both sides run: Create's mixing ones, then the shapeless crafting ones the
/// mixer runs; Create's compacting ones; and the square crafting ones a press asks the game
/// for, 9 before 4 (`compressible`). The mixing and the compacting ones each in a `RecipeTrie`.
pub struct Books {
    pub mixing: Vec<basin::Recipe>,
    pub compacting: Vec<basin::Recipe>,
    squares: Vec<(String, basin::Recipe)>,
    pub mixing_trie: create_engine::recipe_trie::RecipeTrie<usize>,
    pub compacting_trie: create_engine::recipe_trie::RecipeTrie<usize>,
}

impl Books {
    pub fn new() -> Books {
        let recipe = |r: &ManifestRecipe| basin::Recipe {
            id: r.id.clone(),
            ingredients: r.ingredients.clone(),
            results: r
                .results
                .iter()
                .map(|o| Output {
                    item: o.item.clone(),
                    count: o.count,
                    chance: o.chance,
                })
                .collect(),
            time: 0,
            heat: basin::HeatReq::read(Some(r.heat.as_str())),
            fluids: r
                .fluids
                .iter()
                .map(|(f, n)| (Fluid::of_id(f).expect("recipe fluid"), *n))
                .collect(),
            fluid_results: r
                .fluid_results
                .iter()
                .map(|(f, n)| (Fluid::of_id(f).expect("recipe fluid"), *n))
                .collect(),
        };
        let mixing: Vec<basin::Recipe> = contract()
            .mixing
            .iter()
            .chain(&contract().shapeless)
            .map(recipe)
            .collect();
        let compacting: Vec<basin::Recipe> = contract().compacting.iter().map(recipe).collect();
        Books {
            mixing_trie: basin::recipe_trie(&mixing),
            compacting_trie: basin::recipe_trie(&compacting),
            mixing,
            compacting,
            squares: contract()
                .squares
                .iter()
                .map(|s| {
                    (
                        s.item.clone(),
                        basin::Recipe {
                            id: s.id.clone(),
                            ingredients: vec![vec![s.item.clone()]; s.size * s.size],
                            results: vec![Output {
                                item: s.result.clone(),
                                count: s.count,
                                chance: 1.0,
                            }],
                            time: 0,
                            heat: basin::HeatReq::None,
                            fluids: Vec::new(),
                            fluid_results: Vec::new(),
                        },
                    )
                })
                .collect(),
        }
    }

    /// `compressible`: the square crafting recipes of one item, those with the most
    /// ingredients first (`getMatchingRecipes`' order).
    pub fn squares(&self, item: &str) -> impl Iterator<Item = &basin::Recipe> {
        self.squares
            .iter()
            .filter(move |(i, _)| i == item)
            .map(|(_, r)| r)
    }
}

impl Default for Books {
    fn default() -> Books {
        Books::new()
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
fn stack(item: &str, count: u8) -> Stack {
    Stack {
        item: item.into(),
        count,
    }
}
fn drop_on(stack: Stack) -> Action {
    Action::Item {
        input: Input::DropOn { stack },
    }
}
fn insert(slot: u8, stack: Stack) -> Action {
    Action::Capability {
        side: None,
        slot,
        insert: Some(stack),
        extract: None,
        simulate: false,
    }
}
fn extract(slot: u8) -> Action {
    Action::Capability {
        side: None,
        slot,
        insert: None,
        extract: Some(64),
        simulate: false,
    }
}
/// A player's use on the top of the block (`ItemInputs.apply`): `None` an empty hand.
fn use_on(stack: Option<Stack>) -> Action {
    Action::Item {
        input: Input::Use {
            stack,
            face: Face::Up,
            yaw: 0.,
            sneaking: false,
        },
    }
}
fn use_at(tick: u32, pos: Pos, item: &str, face: Face) -> Change {
    Change {
        tick,
        pos,
        state: None,
        motor: None,
        turn: None,
        action: Some(Action::Use {
            item: item.into(),
            face,
            hit: face.point(0.5, 0.5),
            yaw: 0.,
            pitch: 0.,
            sneaking: false,
        }),
    }
}
fn air(tick: u32, pos: Pos) -> Change {
    Change {
        tick,
        pos,
        action: None,
        state: Some("minecraft:air".into()),
        motor: None,
        turn: None,
    }
}

/// What goes into a basin at one time: items (each with the most put in at once) and buckets
/// of fluid emptied into its tanks before them.
struct Batch {
    items: &'static [(&'static str, u8)],
    buckets: &'static [&'static str],
}

const NO_FLUID: &[&str] = &[];
const WATER: &[&str] = &["minecraft:water_bucket"];
const MILK: &[&str] = &["minecraft:milk_bucket"];
const WATER_AND_MILK: &[&str] = &["minecraft:water_bucket", "minecraft:milk_bucket"];

/// A mixing history: what the basin is given, batch by batch, and the heat its recipes need
/// (`HeatCondition`).
struct Mixing {
    batches: &'static [Batch],
    heat: basin::HeatReq,
}

const MIXING: [Mixing; 12] = [
    Mixing {
        batches: &[
            Batch {
                items: &[("minecraft:andesite", 12), ("minecraft:iron_nugget", 5)],
                buckets: NO_FLUID,
            },
            Batch {
                items: &[("minecraft:iron_nugget", 4), ("minecraft:andesite", 9)],
                buckets: NO_FLUID,
            },
        ],
        heat: basin::HeatReq::None,
    },
    Mixing {
        batches: &[
            Batch {
                items: &[("minecraft:andesite", 12), ("create:zinc_nugget", 5)],
                buckets: NO_FLUID,
            },
            Batch {
                items: &[("minecraft:andesite", 20), ("create:zinc_nugget", 3)],
                buckets: NO_FLUID,
            },
        ],
        heat: basin::HeatReq::None,
    },
    Mixing {
        batches: &[
            Batch {
                items: &[("minecraft:copper_ingot", 9), ("create:zinc_ingot", 9)],
                buckets: NO_FLUID,
            },
            Batch {
                items: &[("create:zinc_ingot", 12), ("minecraft:copper_ingot", 5)],
                buckets: NO_FLUID,
            },
        ],
        heat: basin::HeatReq::Heated,
    },
    Mixing {
        batches: &[
            Batch {
                items: &[("minecraft:sugar", 3), ("minecraft:cocoa_beans", 3)],
                buckets: MILK,
            },
            Batch {
                items: &[("minecraft:cocoa_beans", 2), ("minecraft:sugar", 6)],
                buckets: MILK,
            },
        ],
        heat: basin::HeatReq::Heated,
    },
    Mixing {
        batches: &[
            Batch {
                items: &[("create:wheat_flour", 6)],
                buckets: WATER,
            },
            Batch {
                items: &[("create:wheat_flour", 4)],
                buckets: WATER,
            },
        ],
        heat: basin::HeatReq::None,
    },
    Mixing {
        batches: &[
            Batch {
                items: &[("minecraft:dirt", 4)],
                buckets: WATER,
            },
            Batch {
                items: &[("minecraft:wheat", 3)],
                buckets: NO_FLUID,
            },
        ],
        heat: basin::HeatReq::None,
    },
    Mixing {
        batches: &[
            Batch {
                items: &[("create:bar_of_chocolate", 4)],
                buckets: NO_FLUID,
            },
            Batch {
                items: &[("create:bar_of_chocolate", 8)],
                buckets: NO_FLUID,
            },
        ],
        heat: basin::HeatReq::Heated,
    },
    Mixing {
        batches: &[
            Batch {
                items: &[("minecraft:honey_block", 3)],
                buckets: NO_FLUID,
            },
            Batch {
                items: &[("minecraft:honey_block", 5)],
                buckets: NO_FLUID,
            },
        ],
        heat: basin::HeatReq::Heated,
    },
    Mixing {
        batches: &[
            Batch {
                items: &[("minecraft:cobblestone", 5)],
                buckets: NO_FLUID,
            },
            Batch {
                items: &[("minecraft:cobblestone", 9)],
                buckets: NO_FLUID,
            },
        ],
        heat: basin::HeatReq::Superheated,
    },
    Mixing {
        batches: &[
            Batch {
                items: &[("minecraft:oak_leaves", 3)],
                buckets: WATER_AND_MILK,
            },
            Batch {
                items: &[("minecraft:oak_leaves", 4)],
                buckets: WATER,
            },
        ],
        heat: basin::HeatReq::Heated,
    },
    Mixing {
        batches: &[
            Batch {
                items: &[("minecraft:diorite", 6), ("minecraft:cobblestone", 6)],
                buckets: NO_FLUID,
            },
            Batch {
                items: &[("minecraft:cobblestone", 20), ("minecraft:diorite", 3)],
                buckets: NO_FLUID,
            },
        ],
        heat: basin::HeatReq::None,
    },
    Mixing {
        batches: &[
            Batch {
                items: &[("create:shaft", 1), ("minecraft:oak_planks", 4)],
                buckets: NO_FLUID,
            },
            Batch {
                items: &[("minecraft:oak_planks", 3)],
                buckets: NO_FLUID,
            },
        ],
        heat: basin::HeatReq::None,
    },
];

#[derive(Clone, Copy)]
enum Kind {
    Mixing,
    Compacting,
}

/// The family's inputs that no recipe of its kind takes, each with the most put in at once.
fn recipeless(kind: Kind) -> &'static [(&'static str, u8)] {
    static MIXING: std::sync::OnceLock<Vec<(&'static str, u8)>> = std::sync::OnceLock::new();
    static COMPACTING: std::sync::OnceLock<Vec<(&'static str, u8)>> = std::sync::OnceLock::new();
    let (cache, inputs) = match kind {
        Kind::Mixing => (&MIXING, &contract().inputs.mixing),
        Kind::Compacting => (&COMPACTING, &contract().inputs.compacting),
    };
    cache.get_or_init(|| {
        let m = contract();
        let mut recipes: Vec<&ManifestRecipe> = Vec::new();
        match kind {
            Kind::Mixing => recipes.extend(m.mixing.iter().chain(&m.shapeless)),
            Kind::Compacting => {
                recipes.extend(&m.compacting);
            }
        }
        inputs
            .iter()
            .filter(|item| {
                !recipes
                    .iter()
                    .any(|r| r.ingredients.iter().any(|items| items.contains(item)))
                    && (matches!(kind, Kind::Mixing) || !m.squares.iter().any(|s| &s.item == *item))
            })
            .map(|item| (item.as_str(), 9))
            .collect()
    })
}

const MIX_SPEEDS: [i32; 6] = [32, 64, 128, 256, 16, -48];

/// The basin, what stands under it and the depot it may spout into, placed before or after
/// the machine over it: Create's outcome depends on which block entity ticks first.
fn place(sc: &mut Scenario, machine: Vec<Placed>, basin: Vec<Placed>, basin_first: bool) {
    if basin_first {
        sc.blocks.extend(basin);
        sc.blocks.extend(machine);
    } else {
        sc.blocks.extend(machine);
        sc.blocks.extend(basin);
    }
}

/// The motor's speed set to `speed` at `tick` (`Actions.speedSetting`).
fn speed(tick: u32, motor: Pos, state: &str, rpm: i32) -> Change {
    Change {
        tick,
        pos: motor,
        action: None,
        state: Some(state.into()),
        motor: Some(rpm),
        turn: None,
    }
}

/// What stands under the basin (`BasinBlockEntity.getHeatLevelOf`): nothing, a smouldering
/// blaze burner or a passive heater (magma, a lit or an unlit campfire); a burner fed a
/// blaze cake or a creative blaze cake before the items wait or after they wait; a burner
/// placed seething, which its first tick turns down to what its (empty) fuel gives. The
/// blocks, the uses and checkpoints around them.
fn heat_source(
    kind: u32,
    passive: u32,
    burner: Pos,
    first: u32,
    n: u32,
) -> (Vec<Placed>, Vec<Change>, Vec<u32>) {
    let smouldering = "create:blaze_burner[blaze=smouldering,facing=north]";
    let (blocks, events, checkpoints) = match kind {
        1 => (vec![placed(burner, smouldering, None)], vec![], vec![]),
        2 => (
            vec![placed(
                burner,
                match passive {
                    0 => "minecraft:magma_block",
                    1 => {
                        "minecraft:campfire[facing=north,lit=true,signal_fire=false,waterlogged=false]"
                    }
                    _ => {
                        "minecraft:campfire[facing=north,lit=false,signal_fire=false,waterlogged=false]"
                    }
                },
                None,
            )],
            vec![],
            vec![],
        ),
        3 => (
            vec![placed(burner, smouldering, None)],
            vec![
                use_at(n + 2, burner, "create:blaze_cake", Face::Up),
                use_at(n + 4, burner, "create:blaze_cake", Face::Up),
            ],
            (n + 2..n + 6).collect(),
        ),
        4 => (
            vec![placed(burner, smouldering, None)],
            vec![use_at(
                n + 2,
                burner,
                "create:creative_blaze_cake",
                Face::Up,
            )],
            (n + 2..n + 6).collect(),
        ),
        5 => (
            vec![placed(burner, smouldering, None)],
            vec![use_at(first + 30, burner, "create:blaze_cake", Face::Up)],
            (first + 30..first + 34).collect(),
        ),
        _ => (
            vec![placed(
                burner,
                "create:blaze_burner[blaze=seething,facing=north]",
                None,
            )],
            vec![],
            vec![],
        ),
    };
    (blocks, events, checkpoints)
}

/// How a family fills its basin: three batches at `first`, 110 ticks apart, each its
/// buckets emptied two ticks apart before the items, the items two ticks apart, dropped on
/// the basin (`input` 0) or put through its item handler into any slot (1) or the first (2).
struct Feed<'a> {
    basin: Pos,
    first: u32,
    history: &'a [Batch],
    /// What goes in when no recipe takes it.
    plain: &'a Batch,
    nothing: bool,
    input: u32,
}

fn batches(sc: &mut Scenario, r: &mut Rng, feed: &Feed) {
    for k in 0..3 {
        let batch = if feed.nothing {
            feed.plain
        } else {
            &feed.history[k % feed.history.len()]
        };
        let at = feed.first + k as u32 * 110;
        for (i, bucket) in batch.buckets.iter().enumerate() {
            let tick = at - 6 + i as u32 * 2;
            sc.events
                .push(act(tick, feed.basin, use_on(Some(stack(bucket, 1)))));
            sc.checkpoints.extend(tick..tick + 3);
        }
        let mut tick = at;
        for (item, most) in batch.items {
            let count = 1 + r.below(u32::from(*most)) as u8;
            let a = match feed.input {
                0 => drop_on(stack(item, count)),
                1 => insert(r.below(18) as u8, stack(item, count)),
                _ => insert(0, stack(item, count)),
            };
            sc.events.push(act(tick, feed.basin, a));
            tick += 2;
        }
        sc.checkpoints.extend(at - 2..at + 8);
        if k == 0 {
            sc.checkpoints.extend(at + 8..at + 100);
        } else {
            sc.checkpoints.extend((at + 8..at + 100).step_by(3));
        }
    }
}

/// A mechanical mixer over a basin (`MechanicalMixerBlockEntity`, `BasinRecipe`): the basin
/// fed by drops or through its item handler, with buckets of fluid; andesite alloy, brass,
/// chocolate, dough, mud then packed mud, melted chocolate, honey, lava, tea, the shapeless
/// crafting recipes the mixer runs, or nothing a recipe takes. Heat under the basin from a
/// smouldering or a fuelled blaze burner or a passive heater, fuelled before the items or
/// after they wait. Outputs taken through the handler, the basin emptied by hand, the motor
/// stopped or slowed at the bottom of a cycle, the basin removed, the spout shut with the
/// wrench, fluid taken out with a bucket, and a depot the basin spouts into.
pub fn mixing(seed: u64) -> Scenario {
    let mut r = Rng::new(seed);
    let rpm = *r.pick(&MIX_SPEEDS);
    let history = r.pick(&MIXING);
    let nothing = r.chance(0.1);
    let passive = r.below(3);
    // A recipe that needs heat meets a hot enough burner, one not hot enough, or heat
    // arriving after its items wait; a burner placed seething only smoulders.
    let heat = match if nothing {
        basin::HeatReq::None
    } else {
        history.heat
    } {
        basin::HeatReq::None => r.below(3),
        basin::HeatReq::Heated => [3, 4, 5, 1, 6][r.below(5) as usize],
        basin::HeatReq::Superheated => [3, 5, 6, 1][r.below(4) as usize],
    };
    let input = r.below(3);
    let between = r.below(9);
    let spout = r.chance(0.3);
    let basin_first = r.chance(0.5);
    let mut sc = scenario(seed, 0);
    let basin = [0, 6, 0];
    let burner = [0, 5, 0];
    let motor = [-1, 7, 0];
    let motor_state = "create:creative_motor[facing=up]";
    let machine = vec![
        placed(motor, motor_state, Some(rpm)),
        placed(
            [-1, 8, 0],
            "create:cogwheel[axis=y,waterlogged=false]",
            None,
        ),
        placed([0, 8, 0], "create:mechanical_mixer", None),
    ];
    let (under, events, checkpoints) = heat_source(heat, passive, burner, 0, 0);
    let mut around = vec![placed(basin, "create:basin[facing=down]", None)];
    around.extend(under);
    if spout {
        around.push(placed([1, 5, 0], "create:depot[waterlogged=false]", None));
    }
    place(&mut sc, machine, around, basin_first);
    let n = sc.blocks.len() as u32;
    let first = n + 24;
    sc.checkpoints.extend(n..n + 14);
    sc.events.extend(events);
    sc.checkpoints.extend(checkpoints);
    batches(
        &mut sc,
        &mut r,
        &Feed {
            basin,
            first,
            history: history.batches,
            plain: &Batch {
                items: recipeless(Kind::Mixing),
                buckets: NO_FLUID,
            },
            nothing,
            input,
        },
    );
    let at = first + 60;
    match between {
        0 => sc
            .events
            .push(act(at, basin, extract(9 + r.below(9) as u8))),
        1 => sc.events.push(act(at, basin, extract(r.below(9) as u8))),
        2 => sc.events.push(act(at, basin, use_on(None))),
        3 => {
            sc.events.extend([
                speed(first + 30, motor, motor_state, 0),
                speed(first + 50, motor, motor_state, rpm),
            ]);
            sc.checkpoints.extend(first + 30..first + 33);
        }
        7 => {
            sc.events.extend([
                speed(first + 30, motor, motor_state, 16),
                speed(first + 50, motor, motor_state, rpm),
            ]);
            sc.checkpoints.extend(first + 30..first + 33);
        }
        4 => {
            let tick = at + r.below(30);
            sc.events.push(air(tick, basin));
            sc.checkpoints.extend(tick..tick + 4);
        }
        5 => sc
            .events
            .push(use_at(at, basin, "create:wrench", Face::East)),
        6 => sc
            .events
            .push(act(at, basin, use_on(Some(stack("minecraft:bucket", 1))))),
        _ => {}
    }
    if between != 3 && between != 7 {
        sc.checkpoints.extend(at..at + 4);
    }
    finish(&mut sc, first + 220 + 100);
    sc
}

/// A compacting history: what the basin is given, batch by batch.
struct Compacting {
    batches: &'static [Batch],
}

const COMPACTING: [Compacting; 12] = [
    Compacting {
        batches: &[
            Batch {
                items: &[("minecraft:snowball", 16)],
                buckets: NO_FLUID,
            },
            Batch {
                items: &[("minecraft:snowball", 16)],
                buckets: NO_FLUID,
            },
        ],
    },
    Compacting {
        batches: &[
            Batch {
                items: &[("minecraft:oak_planks", 12)],
                buckets: NO_FLUID,
            },
            Batch {
                items: &[("minecraft:oak_planks", 9)],
                buckets: NO_FLUID,
            },
        ],
    },
    Compacting {
        batches: &[
            Batch {
                items: &[("minecraft:clay_ball", 16)],
                buckets: NO_FLUID,
            },
            Batch {
                items: &[("minecraft:clay_ball", 16)],
                buckets: NO_FLUID,
            },
        ],
    },
    Compacting {
        batches: &[
            Batch {
                items: &[("minecraft:coal", 20)],
                buckets: NO_FLUID,
            },
            Batch {
                items: &[("minecraft:coal", 12)],
                buckets: NO_FLUID,
            },
        ],
    },
    Compacting {
        batches: &[
            Batch {
                items: &[("create:zinc_nugget", 32)],
                buckets: NO_FLUID,
            },
            Batch {
                items: &[("create:zinc_nugget", 32)],
                buckets: NO_FLUID,
            },
        ],
    },
    Compacting {
        batches: &[
            Batch {
                items: &[("create:zinc_ingot", 20)],
                buckets: NO_FLUID,
            },
            Batch {
                items: &[("create:zinc_ingot", 20)],
                buckets: NO_FLUID,
            },
        ],
    },
    Compacting {
        batches: &[
            Batch {
                items: &[("minecraft:honey_bottle", 8)],
                buckets: NO_FLUID,
            },
            Batch {
                items: &[("minecraft:honey_bottle", 8)],
                buckets: NO_FLUID,
            },
        ],
    },
    Compacting {
        batches: &[
            Batch {
                items: &[
                    ("minecraft:egg", 2),
                    ("minecraft:sugar", 3),
                    ("create:cinder_flour", 3),
                ],
                buckets: NO_FLUID,
            },
            Batch {
                items: &[
                    ("create:cinder_flour", 3),
                    ("minecraft:egg", 2),
                    ("minecraft:sugar", 3),
                ],
                buckets: NO_FLUID,
            },
        ],
    },
    Compacting {
        batches: &[
            Batch {
                items: &[],
                buckets: &["create:chocolate_bucket"],
            },
            Batch {
                items: &[],
                buckets: &["create:chocolate_bucket"],
            },
        ],
    },
    Compacting {
        batches: &[
            Batch {
                items: &[],
                buckets: &["create:honey_bucket"],
            },
            Batch {
                items: &[],
                buckets: &["create:honey_bucket"],
            },
        ],
    },
    Compacting {
        batches: &[
            Batch {
                items: &[("minecraft:wheat", 16)],
                buckets: NO_FLUID,
            },
            Batch {
                items: &[("minecraft:wheat", 16)],
                buckets: NO_FLUID,
            },
        ],
    },
    Compacting {
        batches: &[
            Batch {
                items: &[("minecraft:andesite", 12)],
                buckets: NO_FLUID,
            },
            Batch {
                items: &[("minecraft:andesite", 20)],
                buckets: NO_FLUID,
            },
        ],
    },
];

const PRESS_SPEEDS: [i32; 5] = [256, 256, 128, 64, 32];

/// A mechanical press over a basin (`MechanicalPressBlockEntity.tryProcessInBasin`,
/// `PressingBehaviour`): snowballs to snow blocks and those to ice, planks, clay, coal, zinc
/// nuggets to ingots to a block, honey bottles with their bottles left, a blaze cake base,
/// chocolate and honey from their buckets, hay, polished andesite, or nothing a recipe
/// takes; the basin fed by drops or through its item handler. Outputs taken through the
/// handler, the basin emptied by hand, the motor stopped with the head down or set to
/// another speed, the basin removed under a running press, the spout shut with the wrench,
/// fluid taken out with a bucket, and a depot the basin spouts into.
pub fn compacting(seed: u64) -> Scenario {
    let mut r = Rng::new(seed);
    let rpm = *r.pick(&PRESS_SPEEDS);
    let history = r.pick(&COMPACTING);
    let nothing = r.chance(0.1);
    let input = r.below(3);
    let between = r.below(9);
    let spout = r.chance(0.3);
    let basin_first = r.chance(0.5);
    let mut sc = scenario(seed, 0);
    let basin = [0, 6, 0];
    let motor = [0, 8, -1];
    let motor_state = "create:creative_motor[facing=south]";
    let machine = vec![
        placed(motor, motor_state, Some(rpm)),
        placed([0, 8, 0], "create:mechanical_press[facing=north]", None),
    ];
    let mut around = vec![placed(basin, "create:basin[facing=down]", None)];
    if spout {
        around.push(placed([1, 5, 0], "create:depot[waterlogged=false]", None));
    }
    place(&mut sc, machine, around, basin_first);
    let n = sc.blocks.len() as u32;
    let first = n + 24;
    sc.checkpoints.extend(n..n + 14);
    batches(
        &mut sc,
        &mut r,
        &Feed {
            basin,
            first,
            history: history.batches,
            plain: &Batch {
                items: recipeless(Kind::Compacting),
                buckets: NO_FLUID,
            },
            nothing,
            input,
        },
    );
    let at = first + 40;
    match between {
        0 => sc
            .events
            .push(act(at, basin, extract(9 + r.below(9) as u8))),
        1 => sc.events.push(act(at, basin, extract(r.below(9) as u8))),
        2 => sc.events.push(act(at, basin, use_on(None))),
        3 => {
            sc.events.extend([
                speed(first + 20, motor, motor_state, 0),
                speed(first + 50, motor, motor_state, rpm),
            ]);
        }
        8 => {
            let other = if rpm >= 128 || r.chance(0.5) {
                rpm / 2
            } else {
                rpm * 2
            };
            sc.events.extend([
                speed(first + 20, motor, motor_state, other),
                speed(first + 50, motor, motor_state, rpm),
            ]);
        }
        4 => {
            let tick = at + r.below(20);
            sc.events.push(air(tick, basin));
            sc.checkpoints.extend(tick..tick + 4);
        }
        5 => sc
            .events
            .push(use_at(at, basin, "create:wrench", Face::East)),
        6 => sc
            .events
            .push(act(at, basin, use_on(Some(stack("minecraft:bucket", 1))))),
        _ => {}
    }
    if between == 3 || between == 8 {
        sc.checkpoints.extend(first + 20..first + 24);
    } else if between != 4 {
        sc.checkpoints.extend(at..at + 4);
    }
    finish(&mut sc, first + 220 + 100);
    sc
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

/// The events in time order and a settling window past the last; the checkpoints likewise.
fn finish(sc: &mut Scenario, last: u32) {
    sc.ticks =
        last.max(sc.events.iter().map(|e| e.tick).max().unwrap_or(0)) + crate::generate::SETTLE;
    sc.events.sort_by_key(|e| e.tick);
    sc.events.retain(|e| e.tick < sc.ticks);
    sc.checkpoints.retain(|t| *t <= sc.ticks);
    sc.checkpoints.sort_unstable();
    sc.checkpoints.dedup();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every family input has its recipes in the manifest: a square has one recipe per item
    /// and size, takes what a compacting basin can hold, and no item is compacted by both a
    /// square and a Create recipe of as many ingredients (the port asks the game for a square,
    /// Create keeps its trie's order).
    #[test]
    fn the_contract_is_consistent() {
        let manifest = contract();
        let books = Books::new();
        let held = |item: &str| {
            manifest.inputs.compacting.iter().any(|i| i == item)
                || manifest
                    .compacting
                    .iter()
                    .any(|r| r.results.iter().any(|o| o.item == item))
        };
        let mut squares = std::collections::BTreeSet::new();
        for s in &manifest.squares {
            assert!(matches!(s.size, 2 | 3));
            assert!(squares.insert((s.item.clone(), s.size)), "{} twice", s.id);
            assert!(
                held(&s.item) || manifest.squares.iter().any(|o| o.result == s.item),
                "{}",
                s.id
            );
            assert_eq!(books.squares(&s.item).count(), 1, "{}", s.id);
        }
        for r in manifest
            .mixing
            .iter()
            .chain(&manifest.shapeless)
            .chain(&manifest.compacting)
        {
            for (i, items) in r.ingredients.iter().enumerate() {
                assert!(!items.is_empty(), "{}: {}", r.id, i);
                for item in items {
                    let made = manifest
                        .mixing
                        .iter()
                        .chain(&manifest.shapeless)
                        .chain(&manifest.compacting)
                        .any(|r| r.results.iter().any(|o| &o.item == item))
                        || manifest.squares.iter().any(|s| &s.result == item);
                    assert!(
                        made || crate::transport::contract()
                            .items
                            .iter()
                            .any(|c| c.id == *item),
                        "{}: {} is neither a contract item nor a recipe's result",
                        r.id,
                        item
                    );
                }
            }
            for fluid in r.fluids.iter().chain(&r.fluid_results) {
                assert!(Fluid::of_id(&fluid.0).is_some(), "{}", r.id);
            }
            for o in &r.results {
                assert!(o.count > 0 && o.chance == 1.0, "{}", r.id);
            }
        }
        for kind in [Kind::Mixing, Kind::Compacting] {
            let inputs = match kind {
                Kind::Mixing => &manifest.inputs.mixing,
                Kind::Compacting => &manifest.inputs.compacting,
            };
            for item in inputs {
                let takes = manifest
                    .mixing
                    .iter()
                    .chain(&manifest.shapeless)
                    .chain(&manifest.compacting)
                    .any(|r| r.ingredients.iter().any(|items| items.contains(item)))
                    || manifest.squares.iter().any(|s| &s.item == item);
                assert!(
                    takes || recipeless(kind).iter().any(|(i, _)| i == item),
                    "{} is neither in a recipe nor known to be recipeless",
                    item
                );
            }
        }
        assert_eq!(
            books.mixing.len(),
            manifest.mixing.len() + manifest.shapeless.len()
        );
        assert_eq!(
            books.mixing[manifest.mixing.len()].id,
            manifest.shapeless[0].id
        );
        assert_eq!(limit("minecraft:snowball"), 16);
        assert_eq!(limit("create:andesite_alloy"), 64);
        assert_eq!(
            remainder("minecraft:water_bucket").as_deref(),
            Some("minecraft:bucket")
        );
        assert_eq!(
            remainder("minecraft:honey_bottle").as_deref(),
            Some("minecraft:glass_bottle")
        );
        assert_eq!(remainder("minecraft:andesite"), None);
        assert_eq!(
            recipeless(Kind::Mixing),
            [("minecraft:gravel", 9)],
            "a family input without a recipe"
        );
        assert_eq!(
            recipeless(Kind::Compacting),
            [("minecraft:dirt", 9), ("minecraft:gravel", 9)]
        );
    }

    #[test]
    fn the_families_build_basins_under_their_machine() {
        for seed in (109..109 + 256 * 40).step_by(256) {
            for sc in [mixing(seed), compacting(seed)] {
                crate::catalogue::scenario(&sc);
                crate::transport::validate_scenario(&sc);
                let basin = sc
                    .blocks
                    .iter()
                    .find(|b| b.state.starts_with("create:basin"))
                    .unwrap();
                let machine = sc
                    .blocks
                    .iter()
                    .find(|b| {
                        b.state.starts_with("create:mechanical_mixer")
                            || b.state.starts_with("create:mechanical_press")
                    })
                    .unwrap();
                assert_eq!(machine.pos, [basin.pos[0], basin.pos[1] + 2, basin.pos[2]]);
                assert!(sc.events.iter().all(|e| e.tick < sc.ticks));
                assert!(sc.checkpoints.iter().all(|t| *t <= sc.ticks));
                let cells = sc.region().len() * (sc.checkpoints.len() + 1);
                assert!(cells <= 131_072, "seed {seed}: {cells} cells");
            }
        }
    }
}
