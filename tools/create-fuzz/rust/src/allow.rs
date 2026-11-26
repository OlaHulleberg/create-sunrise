//! Differences confirmed not to be port bugs, each with its evidence. The comparison skips only
//! the exact pattern described.

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

/// Whether two saved network sizes agree. They always do: the field is left out of the comparison.
///
/// `KineticBlockEntity.networkSize` (saved as `Size`, `KineticBlockEntity.java:226`) is the member
/// count at the entity's last `updateFromNetwork`: its `KineticNetwork.add` (:58-66), or a `sync`
/// of its network. `KineticNetwork.remove` (:92-95) makes whichever member its `HashMap` yields
/// first `networkDirty`; only that member's `KineticBlockEntity.tick` (:120-124) runs
/// `updateNetwork`, which syncs the others only when stress or capacity changed (:123-131). Once
/// a member has left a network that stays, which sizes the others save is not specified.
///
/// Measured on Java 6.0.11 (`b62e837`), the case of `persistence/network-size-case.json` (seed
/// 44387: two belts joined, then split at tick 114), each run alone in the same slot: the motor at
/// -1,8,0 and segments 0,8,0 and 0,8,1 saved 3, 3, 3 in 14 of 20 runs and 2, 2, 3 in 6, the
/// network having 3 members. The port picks the same way: `TorquePropagator::remove` hands the
/// sync to `members.iter().next()` of a `HashMap` seeded per process (`kinetics/networks.rs`), and
/// 12 runs of the port gave 2, 2, 3 eight times and 3, 3, 3 four times.
///
/// Not a port bug: Create's code does not define the value, and no pick of the port's is Java's
/// (identity hashes). A member can leave and rejoin a network within one tick, which no snapshot
/// shows, so the comparison cannot tell where a saved size is still specified, and leaves the
/// field out. The live `networks.sizes` (`KineticNetwork.getSize`) are compared, until a reload
/// seeds the network from such a figure (`cached_network_figures`).
pub fn saved_network_size(_java: u32, _port: u32) -> bool {
    true
}

/// Whether a block's cached network stress and capacity agree, as `(stress, capacity)`. They
/// always do: live (`KineticBlockEntity.capacity`, `.stress`) and saved (`Network.Capacity`,
/// `Network.Stress`), the four are left out of the comparison.
///
/// `KineticBlockEntity.capacity` and `.stress` (`KineticBlockEntity.java:66-67`) hold what its
/// last `updateFromNetwork` (:155-169) wrote: its network's totals when the `KineticNetwork.add`
/// that enrolled it ran (:58-66) or at a later `sync` (:98-101), which only a recalculation that
/// changed a total runs (:107-131); or the zeros of `remove` (:84) and `clearKineticInformation`
/// (:487-496). Which members a change reaches is not specified. `remove` flags whichever member
/// its `HashMap` yields first (:92-95), and a flagged member's `tick` recalculates the network it
/// is in by then (`KineticBlockEntity.java:120-124`). `clearKineticInformation` empties `network`
/// without leaving the network's `members`: the entity stays there as a stale entry the flag can
/// land on, and `add` returns early for it (:59-60) without syncing it.
///
/// Measured on Java 6.0.11 (`b62e837`), the case of `persistence/network-capacity-case.json`
/// (shrunk from seed 1453 of `transmission/sequenced_gearshift`: a motor, a sequenced gearshift,
/// two shafts and a hand crank; two signal pulses, the crank turned at ticks 26 and 50), each run
/// alone in the same slot. At checkpoints 51-54 all five are in one network that holds the motor
/// (524288) and the crank (256): in 24 runs its recalculated stress and capacity read 0 and
/// 524544 for all five every time, while the sequencer's cached `capacity` was 0, its `Size` 4,
/// in 9 runs and 524544, its `Size` 5, in 15. A trace of seeds 429, 1453 and 2477 (36 runs)
/// places the fork at tick 28, when the far side leaves the motor's network: with the stale
/// sequencer first in `members` the network keeps 524544, the crank's 256 included, and with the
/// motor first it is recalculated to 524288; that told the sequencer's figure at tick 51 in 36 of
/// 36. The same case with `create:mechanical_press[facing=north]` for the second shaft saves
/// `stress` 0 in 3 of 8 runs and 256 in 5. With the caches compared, `repeat-java 6 --compact
/// 429 1453 2477` differs in 15 of 30 rounds on the default JVM and in 0 of 30 under
/// `-XX:hashCode=2`, where every identity hash is equal.
///
/// The caches reach a reload too: `KineticBlockEntity.initialize` (:86-92) seeds a loaded network
/// with its first entity's saved `capacity`, `stress` and `networkSize`
/// (`KineticNetwork.initFromTE`, :27-34), so from then on even the live `getSize` and the
/// recalculated totals hold what was saved. Seed 3501 of the same shape (`dead_face`, upright)
/// reloads at tick 47 with the motor's cache fresh in one Java run, size 2 and 524288, and stale
/// in another, 5 and 524544: the live sizes agree until then and read 2 and 5 from tick 48 to
/// 70. A network whose saved figures differ from its recalculated ones is listed in
/// `networks.unspecified` on both sides, and its size and totals are not compared there.
///
/// Not a port bug: Create's code does not define which member a change reaches, and no pick of
/// the port's is Java's (identity hashes). A snapshot cannot tell a stale figure from a current
/// one, so the comparison leaves the caches out and compares `network_stress` and
/// `network_capacity`: the totals recalculated from the network's sources and members, which no
/// member's cache affects.
pub fn cached_network_figures(_java: (f32, f32), _port: (f32, f32)) -> bool {
    true
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
    #[test]
    fn cached_network_figures_are_not_compared() {
        let sc: crate::scenario::Scenario =
            serde_json::from_str(include_str!("persistence/network-capacity-case.json")).unwrap();
        let snap = crate::engine::run(&sc);
        let tick = &snap.checkpoints[&51];
        for key in ["0,8,-1", "0,8,0", "0,8,1", "0,8,2", "0,8,3"] {
            let kinetic = tick.blocks[key].kinetic.as_ref().unwrap();
            assert_eq!(
                (kinetic.network_stress, kinetic.network_capacity),
                (0.0, 524544.0),
                "{key}"
            );
        }
        assert!([0.0, 524544.0].contains(&tick.blocks["0,8,0"].kinetic.as_ref().unwrap().capacity));
        let mut other = snap.clone();
        for block in other.blocks.values_mut().chain(
            other
                .checkpoints
                .values_mut()
                .flat_map(|c| c.blocks.values_mut()),
        ) {
            if let Some(kinetic) = block.kinetic.as_mut() {
                kinetic.stress += 1.;
                kinetic.capacity += 1.;
            }
            if let Some(network) = block.saved.as_mut().and_then(|s| s.network.as_mut()) {
                network.stress += 1.;
                network.capacity += 1.;
            }
        }
        assert!(crate::compare::diff(&other, &snap).is_empty());
        let sequencer = other
            .checkpoints
            .get_mut(&51)
            .unwrap()
            .blocks
            .get_mut("0,8,0")
            .unwrap();
        sequencer.kinetic.as_mut().unwrap().network_capacity *= 0.5;
        assert!(
            crate::compare::diff(&other, &snap)
                .iter()
                .any(|d| d.contains("network capacity"))
        );
    }

    #[test]
    fn saved_network_size_is_not_compared() {
        let sc = serde_json::from_str(include_str!("persistence/network-size-case.json")).unwrap();
        let snap = crate::engine::run(&sc);
        let motor = "-1,8,0";
        assert_eq!(
            snap.networks.as_ref().unwrap().partitions,
            vec![
                ["-1,8,0", "0,8,0", "0,8,1"].map(String::from).to_vec(),
                std::iter::once("-1,8,9".to_owned())
                    .chain((2..=9).map(|z| format!("0,8,{z}")))
                    .collect(),
            ]
        );
        assert!(snap.blocks[motor].saved.as_ref().unwrap().network.is_some());
        let mut other = snap.clone();
        for block in other.blocks.values_mut().chain(
            other
                .checkpoints
                .values_mut()
                .flat_map(|c| c.blocks.values_mut()),
        ) {
            if let Some(network) = block.saved.as_mut().and_then(|s| s.network.as_mut()) {
                network.size += 1;
            }
        }
        assert!(crate::compare::diff(&other, &snap).is_empty());
        other
            .blocks
            .get_mut(motor)
            .unwrap()
            .saved
            .as_mut()
            .unwrap()
            .network
            .as_mut()
            .unwrap()
            .added_capacity *= 2.;
        assert!(
            crate::compare::diff(&other, &snap)
                .iter()
                .any(|d| d.contains("saved kinetic data"))
        );
        other = snap.clone();
        other
            .networks
            .as_mut()
            .unwrap()
            .sizes
            .insert(motor.into(), 99);
        assert!(crate::compare::diff(&other, &snap).contains(&"network sizes differ".into()));
    }
}
