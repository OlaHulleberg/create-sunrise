//! Block entity fields from MIT glue `Subsystem::save_entities` / `load_entity`: what each
//! entity's `saveAdditional` writes, published with its column (`entity_data`).
use super::codec::{self, Compound, Tag};
use crate::scenario::Pos;
use create_engine::Block;
use create_engine::world::CreateWorld;
use std::collections::{BTreeMap, HashMap};

/// glue `Subsystem::live_fields` (`entity_data::write_record`): the countdowns of the record at
/// `p`, from the live world at the last publication's game time `now`.
pub fn live_fields(w: &CreateWorld, p: Pos, now: u64, fields: &mut Compound) {
    match w.block_at(0, p) {
        Some(
            Block::AndesiteFunnel
            | Block::AndesiteBeltFunnel
            | Block::BrassFunnel
            | Block::BrassBeltFunnel,
        ) => match w.saved_funnel_cooldown(0, p, now) {
            Some(0) => {
                fields.remove("TransferCooldown");
            }
            Some(c) => {
                fields.insert("TransferCooldown".into(), Tag::Int(c as i32));
            }
            None => {}
        },
        Some(Block::ItemDrain) => super::fluids::live_drain(w, p, fields),
        // glue/processing/saw.rs `live_saw`, and `live_breaker` (`BlockBreakingKineticBlockEntity.write`).
        Some(Block::MechanicalSaw) => {
            if let Some(s) = w.dims[0].saws.get(&p)
                && let Some(Tag::Compound(c)) = fields.get_mut(SAW_INVENTORY)
            {
                c.insert("ProcessingTime".into(), Tag::Float(s.remaining.to_bits()));
            }
            if let Some(b) = w.dims[0].drills.get(&p) {
                breaker_tag(b, fields);
            }
        }
        Some(Block::MechanicalDrill) => {
            if let Some(b) = w.dims[0].drills.get(&p) {
                breaker_tag(b, fields);
            }
        }
        // glue/processing/persist.rs `live_fields`: the millstone's `Timer`.
        Some(Block::Millstone) => {
            if let Some(m) = w.dims[0].millstones.get(&p) {
                fields.insert("Timer".into(), Tag::Int(m.timer));
            }
        }
        // glue/processing/persist.rs `live_fields`: the press's cycle.
        Some(Block::MechanicalPress) => {
            if let Some(pr) = w.dims[0].presses.get(&p) {
                fields.insert("Running".into(), Tag::Byte(i8::from(pr.running)));
                fields.insert("Mode".into(), Tag::Int(i32::from(pr.mode.index())));
                fields.insert("Finished".into(), Tag::Byte(i8::from(pr.finished)));
                fields.insert("Ticks".into(), Tag::Int(pr.ticks));
            }
        }
        // glue/processing/persist.rs `live_fields`: the mixer's `Running` and `Ticks`.
        Some(Block::MechanicalMixer) => {
            if let Some(m) = w.dims[0].mixers.get(&p) {
                fields.insert("Running".into(), Tag::Byte(i8::from(m.running)));
                fields.insert("Ticks".into(), Tag::Int(m.running_ticks));
            }
        }
        // glue/processing/persist.rs `live_fields`: the deployer as it stands.
        Some(Block::Deployer) => {
            if let Some(d) = w.dims[0].deployers.get(&p) {
                fields.insert(DEPLOYER_STATE.into(), Tag::Compound(deployer_to_tag(d)));
            }
        }
        _ => {}
    }
}

const DEPLOYER_STATE: &str = "DeployerState";

/// glue/processing/persist.rs `deployer_to_tag` of `deployers_to_nbt`: `State`, `Timer`,
/// `Reach` (0: synced, not saved), the held stack's count, `Mode`, `Powered` and its wear, by
/// position; the held stack's item and the fake player's inventory past its hand.
pub fn deployer_to_tag(d: &create_engine::deployer::Deployer) -> Compound {
    let (item, n) = d.held.clone().unwrap_or_default();
    Compound::from([
        (
            "Values".into(),
            Tag::IntArray(vec![
                i32::from(d.state.index()),
                d.timer,
                0,
                i32::from(n),
                i32::from(d.mode.index()),
                i32::from(d.locked),
                d.held_damage as i32,
            ]),
        ),
        ("HeldItem".into(), Tag::String(item)),
        (
            "Owner".into(),
            Tag::IntArray(
                d.owner
                    .map(|u| {
                        std::array::from_fn::<i32, 4, _>(|i| (u >> (96 - 32 * i)) as u32 as i32)
                    })
                    .map(|a| a.to_vec())
                    .unwrap_or_default(),
            ),
        ),
        (
            "Inventory".into(),
            Tag::List(
                d.inventory
                    .iter()
                    .map(|(slot, st)| {
                        Tag::Compound(Compound::from([
                            (
                                "Values".into(),
                                Tag::IntArray(vec![
                                    *slot as i32,
                                    i32::from(st.count),
                                    st.damage as i32,
                                ]),
                            ),
                            ("Item".into(), Tag::String(st.item.clone())),
                        ]))
                    })
                    .collect(),
            ),
        ),
    ])
}

/// `deployer_of`: the deployer a `DeployerState` saved. What is not saved (its breaking, a
/// fist bump) is a new deployer's.
pub fn deployer_of(c: &Compound) -> Option<create_engine::deployer::Deployer> {
    let Some(Tag::IntArray(v)) = c.get("Values") else {
        return None;
    };
    let at = |i: usize| v.get(i).copied().unwrap_or(0);
    let item = match c.get("HeldItem") {
        Some(Tag::String(s)) => s.clone(),
        _ => String::new(),
    };
    let n = at(3).clamp(0, 64) as u8;
    Some(create_engine::deployer::Deployer {
        state: create_engine::deployer::State::from_index(at(0) as u8),
        timer: at(1),
        reach: 0.0,
        held: (!item.is_empty() && n > 0).then_some((item, n)),
        mode: create_engine::deployer::Mode::from_index(at(4) as u8),
        breaking: None,
        locked: at(5) != 0,
        held_damage: at(6).max(0) as u32,
        held_components: Vec::new(),
        fist_bump: false,
        activated: false,
        owner: match c.get("Owner") {
            Some(Tag::IntArray(v)) if v.len() == 4 => Some(
                v.iter()
                    .fold(0u128, |u, i| (u << 32) | u128::from(*i as u32)),
            ),
            _ => None,
        },
        inventory: match c.get("Inventory") {
            Some(Tag::List(l)) => l
                .iter()
                .filter_map(|t| {
                    let Tag::Compound(e) = t else { return None };
                    let (Some(Tag::IntArray(v)), Some(Tag::String(item))) =
                        (e.get("Values"), e.get("Item"))
                    else {
                        return None;
                    };
                    Some((
                        (*v.first()?).max(1) as u32,
                        create_engine::deployer::Stack {
                            item: item.clone(),
                            count: (*v.get(1)?).clamp(0, 64) as u8,
                            damage: v.get(2).map_or(0, |d| (*d).max(0) as u32),
                            components: Vec::new(),
                        },
                    ))
                })
                .collect(),
            _ => Vec::new(),
        },
    })
}

const SAW_INVENTORY: &str = "Inventory";
const RECIPE_INDEX: &str = "RecipeIndex";
const PROGRESS: &str = "Progress";
const NEXT_TICK: &str = "NextTick";
const BREAKING: &str = "Breaking";

/// glue/processing/saw.rs `breaker_tag`: `BlockBreakingKineticBlockEntity.write`.
fn breaker_tag(b: &create_engine::drill::Breaker, out: &mut Compound) {
    out.insert(PROGRESS.into(), Tag::Int(b.progress));
    out.insert(NEXT_TICK.into(), Tag::Int(b.ticks_until));
    match b.breaking_pos {
        Some(at) => {
            out.insert(BREAKING.into(), Tag::IntArray(at.to_vec()));
        }
        None => {
            out.remove(BREAKING);
        }
    }
}

/// glue/processing/saw.rs `save_saws`: `Inventory` and `RecipeIndex` of the saws of `column`
/// holding items or keeping a recipe index (`index`, their `recipeDuration` in `duration`).
pub fn save_saws(
    w: &CreateWorld,
    index: &HashMap<Pos, usize>,
    duration: &HashMap<Pos, f32>,
    column: (i32, i32),
    out: &mut BTreeMap<Pos, Compound>,
) {
    let here = |p: &Pos| (p[0] >> 4, p[2] >> 4) == column;
    for (p, i) in index.iter().filter(|(p, i)| here(p) && **i != 0) {
        out.entry(*p)
            .or_default()
            .insert(RECIPE_INDEX.into(), Tag::Int(*i as i32));
    }
    for (p, s) in w.dims[0].saws.iter().filter(|(p, _)| here(p)) {
        let time = if s.applied {
            20.0
        } else if s.remaining < 0.0 {
            0.0
        } else {
            duration.get(p).copied().unwrap_or(0.0)
        };
        let first = usize::from(s.applied);
        let items = s
            .slots
            .iter()
            .enumerate()
            .map(|(i, (id, n))| {
                Tag::Compound(Compound::from([
                    ("Slot".into(), Tag::Int((first + i) as i32)),
                    ("id".into(), Tag::String(id.clone())),
                    ("count".into(), Tag::Int(i32::from(*n))),
                ]))
            })
            .collect();
        let inventory = Compound::from([
            ("Items".into(), Tag::List(items)),
            ("Size".into(), Tag::Int(32)),
            ("ProcessingTime".into(), Tag::Float(s.remaining.to_bits())),
            ("RecipeTime".into(), Tag::Float(time.to_bits())),
            ("AppliedRecipe".into(), Tag::Byte(i8::from(s.applied))),
        ]);
        let e = out.entry(*p).or_default();
        e.insert(SAW_INVENTORY.into(), Tag::Compound(inventory));
        e.insert(RECIPE_INDEX.into(), Tag::Int(s.index as i32));
    }
}

/// glue/processing/saw.rs `saw_of`: the saw saved in `tag` when it holds stacks, its
/// `recipeIndex` and its `recipeDuration`.
pub fn saw_of(tag: &Compound) -> (Option<create_engine::saw::Saw>, usize, f32) {
    let index = match tag.get(RECIPE_INDEX) {
        Some(Tag::Int(i)) => (*i).max(0) as usize,
        _ => 0,
    };
    let Some(Tag::Compound(inventory)) = tag.get(SAW_INVENTORY) else {
        return (None, index, 0.0);
    };
    let float = |k: &str| match inventory.get(k) {
        Some(Tag::Float(b)) => f32::from_bits(*b),
        _ => 0.0,
    };
    let mut items: Vec<(i32, String, u8)> = match inventory.get("Items") {
        Some(Tag::List(l)) => l
            .iter()
            .filter_map(|t| {
                let Tag::Compound(c) = t else { return None };
                let (Some(Tag::String(id)), Some(Tag::Int(n))) = (c.get("id"), c.get("count"))
                else {
                    return None;
                };
                let slot = match c.get("Slot") {
                    Some(Tag::Int(s)) => *s,
                    _ => 0,
                };
                Some((slot, id.clone(), (*n).clamp(1, 255) as u8))
            })
            .collect(),
        _ => Vec::new(),
    };
    items.sort_by_key(|i| i.0);
    let duration = float("RecipeTime");
    if items.is_empty() {
        return (None, index, duration);
    }
    let s = create_engine::saw::Saw {
        slots: items.into_iter().map(|(_, id, n)| (id, n)).collect(),
        remaining: float("ProcessingTime"),
        applied: matches!(inventory.get("AppliedRecipe"), Some(Tag::Byte(b)) if *b != 0),
        index,
        progress: None,
    };
    (Some(s), index, duration)
}

pub fn save_column(w: &CreateWorld, column: (i32, i32), now: u64) -> BTreeMap<Pos, Compound> {
    let (cx, cz) = column;
    let mut out: BTreeMap<Pos, Compound> = BTreeMap::new();
    for (p, rpm) in w.column_settings(0, cx, cz) {
        out.entry(p)
            .or_default()
            .insert("ScrollValue".into(), Tag::Int(rpm));
    }
    for (c, inv) in w.column_belts(0, cx, cz) {
        out.entry(c)
            .or_default()
            .insert("Inventory".into(), Tag::Compound(belt_to_tag(c, inv)));
    }
    for (p, b) in w.column_brass(0, cx, cz) {
        out.entry(p)
            .or_default()
            .insert("BrassTunnel".into(), Tag::Compound(brass_to_tag(p, b)));
    }
    for (p, at) in w.column_chutes(0, cx, cz) {
        out.entry(p)
            .or_default()
            .insert("ItemPosition".into(), Tag::Float(at.to_bits()));
    }
    for (p, cooldown) in w.column_funnels(0, cx, cz, now) {
        out.entry(p)
            .or_default()
            .insert("TransferCooldown".into(), Tag::Int(cooldown as i32));
    }
    // glue/processing/persist.rs `save_entities`: the millstone's `Timer`.
    for (p, m) in w.dims[0].millstones.iter() {
        if (p[0] >> 4, p[2] >> 4) == column {
            out.entry(*p)
                .or_default()
                .insert("Timer".into(), Tag::Int(m.timer));
        }
    }
    // glue/processing/persist.rs `save_entities`: the press's `Running`, `Mode`, `Finished`
    // and `Ticks`.
    for (p, pr) in w.dims[0].presses.iter() {
        if (p[0] >> 4, p[2] >> 4) == column {
            let e = out.entry(*p).or_default();
            e.insert("Running".into(), Tag::Byte(i8::from(pr.running)));
            e.insert("Mode".into(), Tag::Int(i32::from(pr.mode.index())));
            e.insert("Finished".into(), Tag::Byte(i8::from(pr.finished)));
            e.insert("Ticks".into(), Tag::Int(pr.ticks));
        }
    }
    // glue/processing/persist.rs `save_entities`: the mixer's `Running` and `Ticks`.
    for (p, m) in w.dims[0].mixers.iter() {
        if (p[0] >> 4, p[2] >> 4) == column {
            let e = out.entry(*p).or_default();
            e.insert("Running".into(), Tag::Byte(i8::from(m.running)));
            e.insert("Ticks".into(), Tag::Int(m.running_ticks));
        }
    }
    // glue/processing/saw.rs `save_breakers`: `BlockBreakingKineticBlockEntity.write`.
    for (p, b) in w.dims[0].drills.iter() {
        if (p[0] >> 4, p[2] >> 4) == column {
            breaker_tag(b, out.entry(*p).or_default());
        }
    }
    // glue/processing/persist.rs `save_entities`: each deployer's `DeployerState`.
    for (p, d) in w.column_deployers(0, cx, cz) {
        out.entry(p)
            .or_default()
            .insert(DEPLOYER_STATE.into(), Tag::Compound(deployer_to_tag(&d)));
    }
    super::fluids::save_entities(w, column, &mut out);
    out
}

pub fn load(w: &mut CreateWorld, p: Pos, block: Block, tag: &Compound, now: u64) {
    super::fluids::load(w, p, block, tag);
    match block {
        Block::CreativeMotor | Block::RotationSpeedController => {
            if let Some(Tag::Int(rpm)) = tag.get("ScrollValue") {
                w.load_settings(0, std::iter::once((p, *rpm)));
            }
        }
        Block::Belt => {
            if let Some(inv) = belt_inventory(p, tag) {
                w.read_belt_items(0, p, inv);
            }
        }
        Block::BrassTunnel => {
            if let Some(Tag::Compound(c)) = tag.get("BrassTunnel") {
                let mut c = c.clone();
                c.insert("Pos".into(), Tag::IntArray(p.to_vec()));
                let data = Compound::from([("brass".into(), Tag::List(vec![Tag::Compound(c)]))]);
                for (_, b) in codec::brass_from_nbt(&data) {
                    w.read_brass(0, p, b);
                }
            }
        }
        Block::Chute | Block::SmartChute => {
            if let Some(Tag::Float(bits)) = tag.get("ItemPosition") {
                w.read_chute(0, p, f32::from_bits(*bits));
            }
        }
        Block::AndesiteFunnel
        | Block::AndesiteBeltFunnel
        | Block::BrassFunnel
        | Block::BrassBeltFunnel => {
            if let Some(Tag::Int(cooldown)) = tag.get("TransferCooldown") {
                let cooldown = (*cooldown).max(0) as u32;
                w.read_funnel(0, p, cooldown, now);
            }
        }
        Block::Millstone => {
            let timer = match tag.get("Timer") {
                Some(Tag::Int(t)) => *t,
                _ => 0,
            };
            w.dims[0].millstones.entry(p).or_default().timer = timer;
        }
        // glue/processing/persist.rs `load_entity`: the press's cycle.
        Block::MechanicalPress => {
            let flag = |k: &str| matches!(tag.get(k), Some(Tag::Byte(b)) if *b != 0);
            let int = |k: &str| match tag.get(k) {
                Some(Tag::Int(v)) => *v,
                _ => 0,
            };
            let d = &mut w.dims[0];
            let pr = d.presses.entry(p).or_default();
            pr.running = flag("Running");
            pr.mode = create_engine::press::Mode::from_index(int("Mode").clamp(0, 2) as u8);
            pr.finished = flag("Finished");
            pr.ticks = int("Ticks");
            if pr.running {
                d.press_changes.push(p);
            }
        }
        // glue/processing/saw.rs `load_breaker`: `BlockBreakingKineticBlockEntity.read`.
        Block::MechanicalDrill | Block::MechanicalSaw => {
            let int = |k: &str| match tag.get(k) {
                Some(Tag::Int(v)) => *v,
                _ => 0,
            };
            let breaking = match tag.get(BREAKING) {
                Some(Tag::IntArray(p)) if p.len() == 3 => Some([p[0], p[1], p[2]]),
                _ => None,
            };
            w.dims[0]
                .drills
                .entry(p)
                .or_default()
                .read(int(PROGRESS), int(NEXT_TICK), breaking);
        }
        // glue/processing/persist.rs `load_entity`: the mixer's `Running` and `Ticks`.
        Block::MechanicalMixer => {
            let running = matches!(tag.get("Running"), Some(Tag::Byte(b)) if *b != 0);
            let running_ticks = match tag.get("Ticks") {
                Some(Tag::Int(v)) => *v,
                _ => 0,
            };
            w.dims[0].mixers.insert(
                p,
                create_engine::basin::Mixer {
                    running,
                    running_ticks,
                    ..Default::default()
                },
            );
        }
        _ => {}
    }
}

/// The belt controller's `Inventory` as `load` reads it.
pub fn belt_inventory(c: Pos, tag: &Compound) -> Option<create_engine::belt::Inventory> {
    let Some(Tag::Compound(b)) = tag.get("Inventory") else {
        return None;
    };
    let mut b = b.clone();
    b.insert("pos".into(), Tag::IntArray(c.to_vec()));
    let data = Compound::from([("belts".into(), Tag::List(vec![Tag::Compound(b)]))]);
    codec::belts_from_nbt(&data).pop().map(|(_, inv)| inv)
}

fn belt_to_tag(c: Pos, inv: create_engine::belt::Inventory) -> Compound {
    let Tag::List(mut list) = codec::belts_to_nbt(&[(c, inv)]) else {
        unreachable!()
    };
    let Some(Tag::Compound(mut b)) = list.pop() else {
        unreachable!()
    };
    b.remove("pos");
    b
}

fn brass_to_tag(p: Pos, saved: create_engine::tunnel::SavedBrass) -> Compound {
    let Tag::List(mut list) = codec::brass_to_nbt(&[(p, saved)]) else {
        unreachable!()
    };
    let Some(Tag::Compound(mut c)) = list.pop() else {
        unreachable!()
    };
    c.remove("Pos");
    c
}
