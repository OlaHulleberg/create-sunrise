//! Weak power, as the server measures it from the running binary (`weak_power.json`, audited at
//! startup): whether each block is a redstone conductor (`isRedstoneConductor`), and which blocks
//! are of the classes whose `shouldCheckWeakPower` Create overrides to false. The game reads the
//! power a block receives through it to the blocks beside it (`SignalGetter.getSignal`) when it
//! is a conductor and overrides nothing: the driver composes that from this contract and the
//! port's own list (`create_engine::weak_power::refuses`), as the glue's block part does.
//! Every Create block is in the contract; one whose states or sides disagree (`varies`) conducts
//! nothing here.
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::OnceLock;

#[derive(Deserialize)]
struct Contract {
    blocks: BTreeMap<String, String>,
    #[cfg(test)]
    overrides: Vec<String>,
}

fn contract() -> &'static Contract {
    static CONTRACT: OnceLock<Contract> = OnceLock::new();
    CONTRACT.get_or_init(|| {
        serde_json::from_str(include_str!("weak_power.json")).expect("weak_power.json")
    })
}

/// Whether the block `name` (`create:gearbox`) is a redstone conductor.
pub fn conducts(name: &str) -> bool {
    contract()
        .blocks
        .get(name)
        .is_some_and(|answer| answer == "conducts")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The port's overrides are the binary's, and no others.
    #[test]
    fn the_port_s_overrides_are_the_binary_s() {
        let mut port: Vec<String> = create_engine::Block::ALL
            .iter()
            .filter(|&&b| create_engine::weak_power::refuses(b))
            .map(|b| format!("create:{}", b.name()))
            .collect();
        port.sort();
        assert_eq!(port, contract().overrides);
    }

    /// A conductor checks weak power unless Create overrides it; a shaft, the sources and air
    /// are no conductors.
    #[test]
    fn conductors_and_the_rest() {
        for (id, conducts) in [
            ("create:sequenced_gearshift", true),
            ("create:clutch", true),
            ("create:gearbox", true),
            ("create:andesite_casing", true),
            ("create:shaft", false),
            ("create:cogwheel", false),
            ("create:creative_motor", false),
            ("minecraft:stone", true),
            ("minecraft:air", false),
            ("minecraft:redstone_block", false),
            ("createfuzz:signal", false),
        ] {
            assert_eq!(super::conducts(id), conducts, "{id}");
        }
        assert!(!super::conducts("create:not_a_block"));
        assert_eq!(
            contract()
                .blocks
                .keys()
                .filter(|id| id.starts_with("create:"))
                .count(),
            643
        );
    }
}
