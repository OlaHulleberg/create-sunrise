//! Leaf distances, as the server cascades them. `leaves.json` is the contract: the leaf shapes
//! of the breaking families, the smallest case of #448 and the little shapes that fix each rule
//! of the block ticks, and for every leaf of a shape its `distance` after each level tick and
//! whether one of its ticks was pending, as the server ran it. The Java harness replays the
//! shapes at every start (`java/src/main/java/dev/createfuzz/LeafDistance.java`) and refuses a
//! binary that departs from them; the test here holds the driver's model (`engine/vanilla.rs`)
//! to the same timelines.

use crate::scenario::{Pos, Scenario, key, parse_state};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct Fixture {
    name: String,
    scenario: Scenario,
    leaves: BTreeMap<String, [String; 2]>,
}

fn contract() -> Vec<Fixture> {
    #[derive(Deserialize)]
    struct Contract {
        fixtures: Vec<Fixture>,
    }
    let contract: Contract =
        serde_json::from_str(include_str!("leaves.json")).expect("leaves.json");
    contract.fixtures
}

fn leaf_positions(sc: &Scenario) -> Vec<Pos> {
    let is_leaf =
        |state: &str| crate::breaking::block(&parse_state(state).0).is_some_and(|b| b.leaf);
    let mut positions: Vec<Pos> = sc
        .blocks
        .iter()
        .filter(|b| is_leaf(&b.state))
        .map(|b| b.pos)
        .chain(
            sc.events
                .iter()
                .filter(|e| e.state.as_deref().is_some_and(is_leaf))
                .map(|e| e.pos),
        )
        .collect();
    positions.sort();
    positions.dedup();
    positions
}

#[test]
fn leaf_traces_are_the_server_s() {
    let fixtures = contract();
    for fixture in fixtures {
        let mut sc = fixture.scenario;
        sc.checkpoints = (0..=sc.ticks).collect();
        let positions = leaf_positions(&sc);
        assert_eq!(positions.len(), fixture.leaves.len(), "{}", fixture.name);
        let mut lines = BTreeMap::<Pos, (String, String)>::new();
        let run = crate::engine::Run::new(&sc);
        crate::engine::execute_with(&sc, run, |run, _| {
            for p in &positions {
                let (distance, pending) = run.leaf_cell(*p);
                let line = lines.entry(*p).or_default();
                line.0.push(distance);
                line.1.push(pending);
            }
        });
        let got: BTreeMap<String, [String; 2]> = lines
            .into_iter()
            .map(|(p, (distance, pending))| (key(p), [distance, pending]))
            .collect();
        assert_eq!(got, fixture.leaves, "{}", fixture.name);
    }
}
