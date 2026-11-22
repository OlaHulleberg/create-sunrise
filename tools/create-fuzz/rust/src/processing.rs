//! Processing machines: the millstone, the crushing wheels and the upward saw (see
//! docs/processing-scope.md). Their observation, its comparison, the recipe subset both sides
//! run (`processing.json`, audited by the server) and the scenario families.
use crate::{
    actions::{Action, Face},
    generate::Rng,
    scenario::{Change, Placed, Pos, Scenario},
    transport::{Input, Stack},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One non-empty slot of a machine's inventory.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slot {
    pub item: String,
    pub count: u8,
    pub slot: u8,
}

/// A machine's state (`ProcessingObservations.observe`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Observation {
    pub kind: String,
    /// `MillstoneBlockEntity.timer`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timer: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<Vec<Slot>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outputs: Option<Vec<Slot>>,
    /// `CrushingWheelControllerBlockEntity.crushingspeed` and whether it holds an entity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity: Option<bool>,
    /// A `ProcessingInventory`: its slots, `remainingTime`, `recipeDuration`, `appliedRecipe`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slots: Option<Vec<Slot>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remaining: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipe_index: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tanks: Option<Vec<Option<TankFluid>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tanks: Option<Vec<Option<TankFluid>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub running: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub running_ticks: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub processing_ticks: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TankFluid {
    pub fluid: String,
    pub amount: i32,
}

#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub inputs: BTreeMap<String, Vec<String>>,
    pub recipes: Vec<ManifestRecipe>,
}

#[derive(Debug, Deserialize)]
pub struct ManifestRecipe {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub input: String,
    pub time: i32,
    pub results: Vec<ManifestOutput>,
}

#[derive(Debug, Deserialize)]
pub struct ManifestOutput {
    pub item: String,
    pub count: u8,
    pub chance: f32,
}

pub fn contract() -> &'static Manifest {
    static M: std::sync::OnceLock<Manifest> = std::sync::OnceLock::new();
    M.get_or_init(|| {
        serde_json::from_str(include_str!("processing.json")).expect("processing.json")
    })
}

/// The recipes of `kind` as the engine's book, in the manifest's order.
pub fn book(kind: &str) -> create_engine::processing::Book {
    use create_engine::processing::{Book, Output, Recipe};
    Book::new(
        contract()
            .recipes
            .iter()
            .filter(|r| r.kind == kind)
            .map(|r| Recipe {
                id: r.id.clone(),
                ingredients: vec![vec![r.input.clone()]],
                results: r
                    .results
                    .iter()
                    .map(|o| Output {
                        item: o.item.clone(),
                        count: o.count,
                        chance: o.chance,
                    })
                    .collect(),
                time: r.time,
            })
            .collect(),
    )
}

/// What `level.random` decides in a case (`ProcessingRecipe.rollResults`): the results of the
/// rolled recipes whose input the case offered. An item a rolled recipe makes only by chance may
/// or may not be there (`maybe`); one it makes for sure is there in a count of chance (`counted`).
#[derive(Debug, Default)]
pub struct Rolls {
    counted: std::collections::BTreeSet<String>,
    maybe: std::collections::BTreeSet<String>,
}

impl Rolls {
    pub fn of(a: &crate::scenario::Snapshot, b: &crate::scenario::Snapshot) -> Option<Rolls> {
        let offered: std::collections::BTreeSet<&str> = a
            .item_outcomes
            .iter()
            .chain(&b.item_outcomes)
            .filter_map(|o| o.offered.as_ref().map(|s| s.item.as_str()))
            .collect();
        let active: Vec<&ManifestRecipe> = contract()
            .recipes
            .iter()
            .filter(|r| offered.contains(r.input.as_str()))
            .collect();
        let mut rolls = Rolls::default();
        for r in active
            .iter()
            .filter(|r| r.results.iter().any(|o| o.chance < 1.0))
        {
            for o in &r.results {
                rolls.counted.insert(o.item.clone());
            }
        }
        for r in &active {
            for o in &r.results {
                let sure = r
                    .results
                    .iter()
                    .any(|x| x.item == o.item && x.chance >= 1.0);
                if !sure && rolls.counted.contains(&o.item) {
                    rolls.maybe.insert(o.item.clone());
                }
            }
        }
        for m in &rolls.maybe {
            rolls.counted.remove(m);
        }
        (!rolls.counted.is_empty() || !rolls.maybe.is_empty()).then_some(rolls)
    }

    fn touches(&self, item: &str) -> bool {
        self.counted.contains(item) || self.maybe.contains(item)
    }

    /// Stacks with what chance decides taken out: counts of `counted` items zeroed, `maybe` ones
    /// gone, duplicates merged.
    fn stacks(&self, v: &mut Vec<Stack>) {
        v.retain(|s| !self.maybe.contains(&s.item));
        for s in v.iter_mut() {
            if self.counted.contains(&s.item) {
                s.count = 0;
            }
        }
        v.sort_by(|a, b| (&a.item, a.count).cmp(&(&b.item, b.count)));
        v.dedup();
    }

    fn slots(&self, v: &mut Vec<Slot>, keep_input: bool) {
        if !v
            .iter()
            .any(|x| self.touches(&x.item) && !(keep_input && x.slot == 0))
        {
            return;
        }
        let mut out: Vec<Slot> = v
            .iter()
            .filter(|x| keep_input && x.slot == 0)
            .cloned()
            .collect();
        let mut rest: Vec<Stack> = v
            .iter()
            .filter(|x| !(keep_input && x.slot == 0))
            .map(|x| Stack {
                item: x.item.clone(),
                count: if self.touches(&x.item) { 0 } else { x.count },
            })
            .filter(|x| !self.maybe.contains(&x.item))
            .collect();
        rest.sort_by(|a, b| (&a.item, a.count).cmp(&(&b.item, b.count)));
        rest.dedup();
        out.extend(rest.into_iter().map(|s| Slot {
            item: s.item,
            count: s.count,
            slot: u8::MAX,
        }));
        *v = out;
    }

    /// The snapshot (and its checkpoints) with what chance decides taken out.
    pub fn normalize(&self, s: &crate::scenario::Snapshot) -> crate::scenario::Snapshot {
        let mut s = s.clone();
        self.normalize_in(&mut s);
        s
    }

    fn normalize_in(&self, s: &mut crate::scenario::Snapshot) {
        for o in &mut s.item_outcomes {
            self.stacks(&mut o.taken);
        }
        if let Some(drops) = s.drops.as_mut() {
            drops.retain(|d| !self.maybe.contains(&d.stack.item));
            for d in drops.iter_mut() {
                if self.counted.contains(&d.stack.item) {
                    d.stack.count = 0;
                }
            }
            drops.dedup_by(|a, b| a == b);
        }
        for b in s.blocks.values_mut() {
            if let Some(t) = b.transport.as_mut() {
                let before = (t.items.len(), t.outputs.len());
                let touched = t.items.iter().any(|i| self.touches(&i.stack.item))
                    || t.outputs.iter().any(|o| self.touches(&o.item));
                t.items.retain(|i| !self.maybe.contains(&i.stack.item));
                for i in &mut t.items {
                    if self.counted.contains(&i.stack.item) {
                        i.stack.count = 0;
                    }
                }
                self.stacks(&mut t.outputs);
                if touched || before != (t.items.len(), t.outputs.len()) {
                    t.comparator = None;
                }
                b.payload = None;
            }
            if let Some(p) = b.processing.as_mut() {
                match p.kind.as_str() {
                    "millstone" => {
                        if let Some(o) = p.outputs.as_mut() {
                            self.slots(o, false);
                        }
                    }
                    "crusher" | "saw" => {
                        let applied = p.applied == Some(true);
                        if let Some(o) = p.slots.as_mut() {
                            self.slots(o, !applied);
                        }
                    }
                    _ => {}
                }
            }
        }
        for c in s.checkpoints.values_mut() {
            self.normalize_in(c);
        }
    }
}

/// Whether a recipe that takes `input` rolls its results.
fn rolled(input: &str) -> bool {
    contract()
        .recipes
        .iter()
        .any(|r| r.input == input && r.results.iter().any(|o| o.chance < 1.0))
}

/// Equal machine states. An applied `ProcessingInventory` compares its stacks, not where they
/// sit: Create keeps results after slot 0 and an uncut input in it, the port keeps a list. An
/// empty saw's time is not compared: Create leaves it applied with none left after handing its
/// results to a belt or depot (`SawBlockEntity.tick`), the port forgets it, and every insert
/// sets it again (`insertItem` clears, `start`).
pub fn equal(java: Option<&Observation>, rust: Option<&Observation>) -> bool {
    let unslotted = |o: Option<&Observation>| {
        o.cloned().map(|mut o| {
            if o.kind == "saw" && o.slots.as_ref().is_some_and(Vec::is_empty) {
                o.remaining = None;
                o.duration = None;
                o.applied = None;
            }
            if o.applied == Some(true)
                && let Some(s) = o.slots.as_mut()
            {
                for x in s.iter_mut() {
                    x.slot = 0;
                }
                s.sort_by(|a, b| (&a.item, a.count).cmp(&(&b.item, b.count)));
            }
            o
        })
    };
    unslotted(java) == unslotted(rust)
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
fn drop_on(item: &str, count: u8) -> Action {
    Action::Item {
        input: Input::DropOn {
            stack: Stack {
                item: item.into(),
                count,
            },
        },
    }
}
fn insert(side: Option<Face>, slot: u8, item: &str, count: u8) -> Action {
    Action::Capability {
        side,
        slot,
        insert: Some(Stack {
            item: item.into(),
            count,
        }),
        extract: None,
        simulate: false,
    }
}
fn extract(side: Option<Face>, slot: u8, count: u8) -> Action {
    Action::Capability {
        side,
        slot,
        insert: None,
        extract: Some(count),
        simulate: false,
    }
}
fn empty_use(face: Face) -> Action {
    Action::Item {
        input: Input::Use {
            stack: None,
            face,
            yaw: 0.,
            sneaking: false,
        },
    }
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
fn inputs(machine: &str) -> &'static [String] {
    &contract().inputs[machine]
}

const SPEEDS: [i32; 7] = [8, 16, 32, 64, 128, 256, -48];

/// A millstone on a motor (`MillstoneBlockEntity`): stacks dropped on it and put into its
/// handler (slot 0 only takes what it can mill, outputs refuse), outputs taken through the
/// handler or by a player's empty hand (`MillstoneBlock.useItemOn`, the input when there are no
/// outputs), the motor stopped and restarted, the millstone broken with what it holds.
pub fn millstone(seed: u64) -> Scenario {
    let mut r = Rng::new(seed);
    let mut sc = scenario(seed, 420);
    let mill = [0, 8, 0];
    let rpm = *r.pick(&SPEEDS);
    sc.blocks.push(placed(mill, "create:millstone", None));
    sc.blocks.push(placed(
        [0, 7, 0],
        "create:creative_motor[facing=up]",
        Some(rpm),
    ));
    let mut tick = 10;
    for _ in 0..3 + r.below(4) {
        let item = r.pick(inputs("millstone")).as_str();
        let count = 1 + r.below(if rolled(item) { 6 } else { 20 }) as u8;
        let a = match r.below(5) {
            0 | 1 => drop_on(item, count),
            2 => insert(None, 0, item, count),
            3 => insert(Some(Face::North), 1 + r.below(9) as u8, item, count),
            _ => insert(Some(Face::Up), 0, item, count),
        };
        sc.events.push(act(tick, mill, a));
        sc.checkpoints.extend(tick..tick + 4);
        tick += 15 + r.below(60);
        match r.below(6) {
            0 => sc
                .events
                .push(act(tick, mill, extract(None, 1 + r.below(9) as u8, 64))),
            1 => sc.events.push(act(tick, mill, extract(None, 0, 64))),
            2 => sc.events.push(act(tick, mill, empty_use(Face::Up))),
            3 => sc.events.push(act(
                tick,
                [0, 7, 0],
                Action::SpeedSetting {
                    row: i32::from(r.chance(0.5)),
                    value: *r.pick(&[0, 4, 16, 96]),
                },
            )),
            _ => {}
        }
        tick += 1 + r.below(30);
    }
    if r.chance(0.3) {
        sc.events.push(Change {
            tick: tick.min(sc.ticks - 10),
            pos: mill,
            action: None,
            state: Some("minecraft:air".into()),
            motor: None,
            turn: None,
        });
    }
    sc.checkpoints.retain(|t| *t < sc.ticks);
    sc.checkpoints.extend((60..sc.ticks).step_by(20));
    sc.checkpoints.sort_unstable();
    sc.checkpoints.dedup();
    sc
}

/// Two crushing wheels turning against each other and the controller between them
/// (`CrushingWheelBlock.updateControllers`, `CrushingWheelControllerBlockEntity`): along x or
/// z, horizontal wheels over a depot or nothing, both turned one way (no controller crushes)
/// or a wheel removed; stacks put into the controller's handler, crushed (crushing, else
/// milling, else lost) and let out onto the depot or as items.
pub fn crushing(seed: u64) -> Scenario {
    let mut r = Rng::new(seed);
    let mut sc = scenario(seed, 480);
    let rpm = *r.pick(&[16, 32, 64, 128, 256]);
    let along_x = r.chance(0.5);
    // Wheels on a vertical axis beside the controller would let it out sideways; the wheels'
    // axis here is horizontal, so the controller faces down (`updateControllers`).
    let (axis, a, b, motor_face) = if along_x {
        ("z", [-1, 8, 0], [1, 8, 0], "north")
    } else {
        ("x", [0, 8, -1], [0, 8, 1], "west")
    };
    let behind = |p: Pos| {
        if along_x {
            [p[0], p[1], p[2] + 1]
        } else {
            [p[0] + 1, p[1], p[2]]
        }
    };
    let wheel = format!("create:crushing_wheel[axis={axis}]");
    sc.blocks.push(placed(a, &wheel, None));
    sc.blocks.push(placed(b, &wheel, None));
    let same = r.chance(0.15);
    sc.blocks.push(placed(
        behind(a),
        &format!("create:creative_motor[facing={motor_face}]"),
        Some(rpm),
    ));
    sc.blocks.push(placed(
        behind(b),
        &format!("create:creative_motor[facing={motor_face}]"),
        Some(if same { rpm } else { -rpm }),
    ));
    let controller = [0, 8, 0];
    if r.chance(0.6) {
        sc.blocks
            .push(placed([0, 7, 0], "create:depot[waterlogged=false]", None));
    }
    let mut tick = 20;
    for _ in 0..2 + r.below(3) {
        let item = r.pick(inputs("crusher")).as_str();
        let count = 1 + r.below(if rolled(item) { 4 } else { 16 }) as u8;
        sc.events.push(act(
            tick,
            controller,
            insert(None, r.below(2) as u8, item, count),
        ));
        sc.checkpoints.extend(tick..tick + 3);
        tick += 40 + r.below(120);
        if r.chance(0.2) {
            sc.events.push(act(tick, [0, 7, 0], extract(None, 0, 64)));
        }
    }
    if r.chance(0.25) {
        sc.events.push(Change {
            tick: tick.min(sc.ticks - 20),
            pos: b,
            action: None,
            state: Some("minecraft:air".into()),
            motor: None,
            turn: None,
        });
    }
    sc.checkpoints.retain(|t| *t < sc.ticks);
    sc.checkpoints.extend((40..sc.ticks).step_by(10));
    sc.checkpoints.sort_unstable();
    sc.checkpoints.dedup();
    sc
}

/// A saw facing up (`SawBlockEntity` processing): an item dropped on it or put into its
/// handler (one at a time: `bulkCutting` off), cut by its cutting recipe (10 ticks and through
/// without one), and thrown the way it turns: onto a depot there, or out as items. Its speed
/// changes sign mid-cut, the saw stops, or is broken holding what it cut.
pub fn saw(seed: u64) -> Scenario {
    let mut r = Rng::new(seed);
    let mut sc = scenario(seed, 360);
    let saw = [0, 8, 0];
    let along_first = r.chance(0.5);
    sc.blocks.push(placed(
        saw,
        &format!("create:mechanical_saw[axis_along_first={along_first},facing=up,flipped=false]"),
        None,
    ));
    // Facing up, its shaft runs along x with `axis_along_first`, else along z
    // (`DirectionalAxisKineticBlock.getRotationAxis`).
    let (motor, facing) = if along_first {
        ([-1, 8, 0], "east")
    } else {
        ([0, 8, -1], "south")
    };
    let rpm = *r.pick(&[24, 48, 96, 256, -32, -128]);
    sc.blocks.push(placed(
        motor,
        &format!("create:creative_motor[facing={facing}]"),
        Some(rpm),
    ));
    let m = create_engine::saw::movement(along_first, rpm as f32);
    if r.chance(0.6) {
        sc.blocks.push(placed(
            [m[0], 8, m[2]],
            "create:depot[waterlogged=false]",
            None,
        ));
    }
    let mut tick = 10;
    for _ in 0..3 + r.below(4) {
        let item = r.pick(inputs("saw")).as_str();
        let a = if r.chance(0.6) {
            drop_on(item, 1 + r.below(8) as u8)
        } else {
            insert(Some(Face::Up), 0, item, 1 + r.below(8) as u8)
        };
        sc.events.push(act(tick, saw, a));
        sc.checkpoints.extend(tick..tick + 4);
        tick += 8 + r.below(50);
        if r.chance(0.2) {
            sc.events.push(act(
                tick,
                motor,
                Action::SpeedSetting {
                    row: i32::from(rpm < 0),
                    value: rpm.unsigned_abs() as i32,
                },
            ));
            tick += 1;
        }
    }
    if r.chance(0.2) {
        sc.events.push(Change {
            tick: tick.min(sc.ticks - 10),
            pos: saw,
            action: None,
            state: Some("minecraft:air".into()),
            motor: None,
            turn: None,
        });
    }
    sc.checkpoints.retain(|t| *t < sc.ticks);
    sc.checkpoints.extend((30..sc.ticks).step_by(10));
    sc.checkpoints.sort_unstable();
    sc.checkpoints.dedup();
    sc
}
