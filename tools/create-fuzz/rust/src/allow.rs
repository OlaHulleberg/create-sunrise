//! Differences confirmed not to be port bugs, each with its evidence. The comparison skips only
//! the exact pattern described.

use crate::scenario::Snapshot;

/// Exact nearest-direction tie evidence, propagated only through placement/wrench
/// orientation and the tied chain component's computed shape properties.
/// Java 1.21.1: yaw 45, pitch 0, radians -0.7853982, sin bits 3207922931,
/// cos bits 1060439283 (equal magnitudes), nearest SOUTH. Port cosine bits
/// 1060438146 select WEST. No approximate angles or horizontal-direction APIs qualify.
///
/// Not a port bug: the tie is decided by vanilla's `Mth` sine table, not by Create's code. The
/// port's `place::mth_sin`/`mth_cos` (`create/src/engine/src/place.rs`) use the same table
/// and indexing as the game it runs in: Sunrise's `sunrise-core/src/geometry/trig.rs`
/// (`SCALE = 10430.378350470453`, entries `(i / SCALE).sin() as f32`, double-to-long index,
/// "generated from the 26.3 table formula"), and Sunrise's own `level/place.rs`
/// `ordered_by_nearest` reads it. The Java host runs Minecraft 1.21.1, whose table ties at
/// this angle (bits measured above). Matching 1.21.1 here would make the port disagree with
/// the game it ships in, so the difference is between vanilla versions, not a port bug.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum NearestTie {
    Yaw45Pitch0,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct MeasuredTrigState {
    #[serde(default)]
    pub tie: Option<NearestTie>,
    pub java: String,
    pub port: String,
    #[serde(default)]
    pub properties: Vec<String>,
}

pub fn trig_state(java: &str, port: &str, measured: Option<&MeasuredTrigState>) -> bool {
    let normalize = |s: &str| {
        let (name, props) = crate::scenario::parse_state(s);
        crate::scenario::format_state(&name, &props)
    };
    measured.is_some_and(|m| {
        let (jn, jp) = crate::scenario::parse_state(&m.java);
        let (rn, rp) = crate::scenario::parse_state(&m.port);
        m.tie == Some(NearestTie::Yaw45Pitch0)
            && jn == rn
            && !m.properties.is_empty()
            && m.properties
                .iter()
                .all(|p| ["axis", "facing", "part", "axis_along_first"].contains(&p.as_str()))
            && jp.len() == rp.len()
            && jp.iter().all(|(k, v)| {
                rp.iter()
                    .any(|(rk, rv)| rk == k && (rv == v || m.properties.contains(k)))
            })
            && normalize(java) == normalize(&m.java)
            && normalize(port) == normalize(&m.port)
    })
}

#[cfg(test)]
mod measured_tests {
    #[test]
    fn only_measured_state_pair_is_allowed() {
        let m = super::MeasuredTrigState {
            tie: Some(super::NearestTie::Yaw45Pitch0),
            properties: vec!["axis".into()],
            java: "create:shaft[axis=z,waterlogged=false]".into(),
            port: "create:shaft[axis=x,waterlogged=false]".into(),
        };
        assert!(super::trig_state(&m.java, &m.port, Some(&m)));
        assert!(!super::trig_state(
            "create:shaft[axis=y,waterlogged=false]",
            &m.port,
            Some(&m)
        ));
        assert!(!super::trig_state(&m.java, &m.port, None));
        let sc = serde_json::from_str(
            r#"{
  "version": 3,
  "ticks": 140,
  "blocks": [
    {
      "pos": [
        2,
        7,
        0
      ],
      "state": "minecraft:stone"
    }
  ],
  "checkpoints": [
    10,
    73
  ],
  "events": [
    {
      "tick": 4,
      "pos": [
        2,
        7,
        0
      ],
      "action": {
        "kind": "use",
        "item": "create:shaft",
        "face": "up",
        "hit": [
          0.5,
          1.0,
          0.5
        ],
        "yaw": 45.0,
        "pitch": 0.0,
        "sneaking": false
      }
    },
    {
      "tick": 70,
      "pos": [
        2,
        8,
        0
      ],
      "action": {
        "kind": "use",
        "item": "create:wrench",
        "face": "up",
        "hit": [
          0.5,
          1.0,
          0.5
        ],
        "yaw": 0.0,
        "pitch": 0.0,
        "sneaking": false
      }
    }
  ]
}
"#,
        )
        .unwrap();
        let snap = crate::engine::run(&sc);
        let r = &snap.blocks["2,8,0"];
        let m = r.measured_trig.as_ref().unwrap();
        assert_eq!(m.java, "create:shaft[axis=x,waterlogged=false]");
        assert_eq!(m.port, "create:shaft[axis=z,waterlogged=false]");
        let mut j = snap.clone();
        j.blocks.get_mut("2,8,0").unwrap().state = m.java.clone();
        assert!(crate::compare::diff(&j, &snap).is_empty());
        j.networks
            .as_mut()
            .unwrap()
            .sizes
            .insert("2,8,0".into(), 99);
        assert!(crate::compare::diff(&j, &snap).contains(&"network sizes differ".into()));
    }
}

/// A documented API gap, not a port bug: API_GAPS.md "A per-entity tag (or an item entity's own
/// data) a block's collision can read, so an item entity passes a block that spawned it".
///
/// `CrushingWheelControllerBlockEntity.tick` (`:124`-`:130`, `:186`-`:194`) lets results out of
/// a controller facing down at `outPos`, the block's middle plus `.55f` down: y
/// `c + 0.5 - f64::from(0.55f32)` (`c + 0.949999988…`), motion zero, tagged
/// `BypassCrushingWheel` so the controller's collision for items lets them fall through it. The
/// port cannot tag the entity for its collision, so `crusher::output` starts them clear of the
/// controller, a quarter lower at `c - 0.3`, motion zero (`placed: exact`). Measured on seeds
/// 1757, 2269, 3805 and 5085 (`processing/crushing`): their only differences were these drops.
///
/// A port drop is moved to the Java drop it pairs with only when both are exactly that: the
/// same tick and stack, x and z the controller's middle, the two heights above, no motion, and a
/// `create:crushing_wheel_controller` facing down at that block in the Java snapshot or one of
/// its checkpoints. Each Java drop pairs once.
pub fn crusher_bypass(java: &Snapshot, rust: &mut Snapshot, controllers: &Controllers) {
    let same = |a: f64, b: f64| (a - b).abs() < 1e-9;
    let still = |m: &[f64; 3]| m.iter().all(|v| *v == 0.0);
    let out = |c: &[i32; 3], y: f64, p: &[f64; 3]| {
        same(p[0], f64::from(c[0]) + 0.5) && same(p[1], y) && same(p[2], f64::from(c[2]) + 0.5)
    };
    if let (Some(j), Some(r)) = (&java.drops, rust.drops.as_mut()) {
        let mut used = vec![false; j.len()];
        for d in r.iter_mut() {
            if d.placed != Some(crate::transport::Placed::Exact) || !still(&d.motion) {
                continue;
            }
            let Some(c) = controllers
                .iter()
                .find(|c| out(c, f64::from(c[1]) - 0.3, &d.pos))
            else {
                continue;
            };
            let java_y = f64::from(c[1]) + 0.5 - f64::from(0.55f32);
            if let Some(i) = (0..j.len()).find(|&i| {
                !used[i]
                    && j[i].placed.is_none()
                    && j[i].tick == d.tick
                    && j[i].stack == d.stack
                    && still(&j[i].motion)
                    && out(c, java_y, &j[i].pos)
            }) {
                used[i] = true;
                d.pos = j[i].pos;
                d.motion = j[i].motion;
            }
        }
    }
    for (t, r) in rust.checkpoints.iter_mut() {
        if let Some(j) = java.checkpoints.get(t) {
            crusher_bypass(j, r, controllers);
        }
    }
}

pub type Controllers = std::collections::BTreeSet<[i32; 3]>;

/// The blocks where a snapshot or one of its checkpoints has a crushing wheel controller facing
/// down.
pub fn downward_controllers(s: &Snapshot) -> Controllers {
    let mut out = Controllers::new();
    for snapshot in std::iter::once(s).chain(s.checkpoints.values()) {
        for (k, b) in &snapshot.blocks {
            let (name, props) = crate::scenario::parse_state(&b.state);
            if name == "create:crushing_wheel_controller"
                && props.iter().any(|(k, v)| k == "facing" && v == "down")
            {
                let p: Vec<i32> = k.split(',').filter_map(|v| v.parse().ok()).collect();
                if let [x, y, z] = p[..] {
                    out.insert([x, y, z]);
                }
            }
        }
    }
    out
}

/// A documented API gap, not a port bug: API_GAPS.md "The block or position in
/// `BlockEntityPart::can_place_item`".
///
/// `MillstoneBlockEntity.MillstoneInventoryHandler.insertItem` (`:236`) refuses a stack for the
/// input slot that `isItemValid` (`:229`) rejects: `canProcess` (`:211`), no milling recipe takes
/// it. The port's container has no per-item rule for a millstone, since the hook sees the
/// entity's slots but not its block, so the port's handler takes it. Measured on seed 2765
/// (`processing/millstone`): 19 dirt put into slot 0 through the handler, refused whole in
/// Java and taken in the port, which then holds it and waits (timer 100) while Java's millstone
/// stays empty.
///
/// What the port does after taking it follows from holding the stack, so a case is explained
/// only when this is its first difference: every earlier checkpoint is equal, and at the first
/// that differs (or the end) the first differing item outcome is a handler insert at a
/// `create:millstone` of a millstone input that the audited manifest (`processing.json`) gives
/// no milling recipe, refused whole by Java and not by the port, and the snapshot is equal once
/// that outcome, those after it and the millstone's own observation are taken as Java's.
pub fn millstone_unmillable(java: &Snapshot, rust: &Snapshot) -> bool {
    let mut pairs: Vec<(&Snapshot, &Snapshot)> = Vec::new();
    for (t, j) in &java.checkpoints {
        let Some(r) = rust.checkpoints.get(t) else {
            return false;
        };
        pairs.push((j, r));
    }
    let strip = |s: &Snapshot| Snapshot {
        checkpoints: Default::default(),
        ..s.clone()
    };
    let (java_final, rust_final) = (strip(java), strip(rust));
    pairs.push((&java_final, &rust_final));
    let Some((j, r)) = pairs
        .into_iter()
        .find(|(j, r)| !crate::compare::diff_exact(j, r).is_empty())
    else {
        return false;
    };
    let Some(i) = (0..j.item_outcomes.len().max(r.item_outcomes.len()))
        .find(|&i| j.item_outcomes.get(i) != r.item_outcomes.get(i))
    else {
        return false;
    };
    let (Some(jo), Some(ro)) = (j.item_outcomes.get(i), r.item_outcomes.get(i)) else {
        return false;
    };
    let key = format!("{},{},{}", jo.pos[0], jo.pos[1], jo.pos[2]);
    let Some(offered) = jo.offered.as_ref() else {
        return false;
    };
    let manifest = crate::processing::contract();
    let unmillable = manifest.inputs["millstone"].contains(&offered.item)
        && !manifest
            .recipes
            .iter()
            .any(|x| x.kind == "milling" && x.input == offered.item);
    let pattern = unmillable
        && j.blocks
            .get(&key)
            .is_some_and(|b| b.state == "create:millstone")
        && jo.handler == Some(true)
        && ro.handler == Some(true)
        && ro.pos == jo.pos
        && ro.offered == jo.offered
        && jo.remainder.as_ref() == Some(offered)
        && ro.remainder != jo.remainder
        && jo.taken.is_empty()
        && ro.taken.is_empty();
    if !pattern {
        return false;
    }
    let mut r = r.clone();
    r.item_outcomes = j.item_outcomes.clone();
    if let (Some(rb), Some(jb)) = (r.blocks.get_mut(&key), j.blocks.get(&key)) {
        rb.processing = jb.processing.clone();
    }
    crate::compare::diff_exact(j, &r).is_empty()
}

#[cfg(test)]
mod api_gap_tests {
    use crate::observation::BlockSnap;
    use crate::processing::{Observation, Slot};
    use crate::scenario::Snapshot;
    use crate::transport::{DroppedItem, Outcome, Placed, Stack};

    fn stack(item: &str, count: u8) -> Stack {
        Stack {
            item: item.into(),
            count,
        }
    }
    fn block(state: &str) -> BlockSnap {
        BlockSnap {
            state: state.into(),
            ..BlockSnap::default()
        }
    }
    fn drop_at(y: f64, placed: Option<Placed>) -> DroppedItem {
        DroppedItem {
            tick: 477,
            stack: stack("minecraft:gravel", 6),
            pos: [0.5, y, 0.5],
            motion: [0.0; 3],
            placed,
        }
    }
    fn crushing(controller: &str, y: f64) -> (Snapshot, Snapshot) {
        let mut java = Snapshot {
            drops: Some(vec![drop_at(7.949_999_988_079_071, None)]),
            ..Snapshot::default()
        };
        java.blocks.insert("0,8,0".into(), block(controller));
        let mut rust = Snapshot {
            drops: Some(vec![drop_at(y, Some(Placed::Exact))]),
            ..Snapshot::default()
        };
        rust.blocks.insert("0,8,0".into(), block(controller));
        (java, rust)
    }

    #[test]
    fn crushing_results_let_out_clear_of_a_downward_controller_pair_with_java_s() {
        let down = "create:crushing_wheel_controller[facing=down,valid=true]";
        let (j, r) = crushing(down, 7.7);
        assert!(crate::compare::diff(&j, &r).is_empty());
        let (j, r) = crushing(down, 7.75);
        assert!(crate::compare::diff(&j, &r)[0].starts_with("drops:"));
        let (j, r) = crushing(
            "create:crushing_wheel_controller[facing=north,valid=true]",
            7.7,
        );
        assert!(crate::compare::diff(&j, &r)[0].starts_with("drops:"));
        let (mut j, r) = crushing(down, 7.7);
        j.drops.as_mut().unwrap()[0].motion[0] = 0.1;
        assert!(crate::compare::diff(&j, &r)[0].starts_with("drops:"));
    }

    fn millstone(item: &str, input: Vec<Slot>, timer: i32, remainder: Option<Stack>) -> Snapshot {
        let mut s = Snapshot {
            item_outcomes: vec![Outcome {
                pos: [0, 8, 0],
                offered: Some(stack(item, 19)),
                remainder,
                taken: vec![],
                handler: Some(true),
            }],
            ..Snapshot::default()
        };
        let mut b = block("create:millstone");
        b.processing = Some(Observation {
            kind: "millstone".into(),
            timer: Some(timer),
            input: Some(input),
            outputs: Some(vec![]),
            ..Observation::default()
        });
        s.blocks.insert("0,8,0".into(), b);
        s
    }

    #[test]
    fn an_unmillable_stack_the_port_s_millstone_takes_is_the_documented_gap() {
        let held = |item: &str| {
            vec![Slot {
                item: item.into(),
                count: 19,
                slot: 0,
            }]
        };
        let refused = |item: &str| millstone(item, vec![], 0, Some(stack(item, 19)));
        let taken = |item: &str| millstone(item, held(item), 100, None);
        let dirt = "minecraft:dirt";
        let mut j = refused(dirt);
        let mut r = taken(dirt);
        assert!(!crate::compare::diff(&j, &r).is_empty());
        assert!(crate::compare::api_gap(&j, &r).is_some());
        j.checkpoints.insert(166, refused(dirt));
        r.checkpoints.insert(166, taken(dirt));
        assert!(crate::compare::api_gap(&j, &r).is_some());
        let mut earlier = r.clone();
        earlier.checkpoints.insert(100, taken(dirt));
        let mut java_earlier = j.clone();
        java_earlier.checkpoints.insert(100, refused(dirt));
        java_earlier
            .checkpoints
            .get_mut(&100)
            .unwrap()
            .item_outcomes
            .clear();
        assert!(crate::compare::api_gap(&java_earlier, &earlier).is_none());
        let mut elsewhere = r.clone();
        elsewhere
            .checkpoints
            .get_mut(&166)
            .unwrap()
            .blocks
            .insert("1,8,0".into(), block("minecraft:stone"));
        assert!(crate::compare::api_gap(&j, &elsewhere).is_none());
        let wheat = "minecraft:wheat";
        assert!(crate::compare::api_gap(&refused(wheat), &taken(wheat)).is_none());
        let mut not_a_millstone = taken(dirt);
        let mut java_crusher = refused(dirt);
        for s in [&mut not_a_millstone, &mut java_crusher] {
            s.blocks.get_mut("0,8,0").unwrap().state =
                "create:crushing_wheel_controller[facing=down,valid=true]".into();
        }
        assert!(crate::compare::api_gap(&java_crusher, &not_a_millstone).is_none());
    }
}
