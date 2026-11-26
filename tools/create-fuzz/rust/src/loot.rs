//! What a broken block drops: its loot table (`create/data/create/loot_table/blocks/<name>.json`,
//! Create's generated data), as the host rolls it for `destroyBlock(pos, true)`. Only the
//! tables of Create's blocks, and only what they use: one roll of plain item entries under
//! `survives_explosion`, `match_block` and `inverted` conditions.
use serde_json::Value;

/// A property the port declares for Create's block (`create/data/create/sunrise/block/<name>.json`).
fn declared(block: &str, key: &str) -> Option<serde_json::Value> {
    let name = block.strip_prefix("create:")?;
    let path =
        crate::defaults::checkout().join(format!("create/data/create/sunrise/block/{name}.json"));
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    doc["properties"].get(key).cloned()
}

/// Whether a player's empty hand breaks the block in `state` without its drops
/// (`requiresCorrectToolForDrops`): the port's block data for Create's blocks, and for the
/// vanilla blocks cases place what a hand-broken one was seen to drop. None for others.
pub fn requires_tool(state: &str) -> Option<bool> {
    let (block, _) = crate::scenario::parse_state(state);
    if block.starts_with("createfuzz:") {
        return Some(false);
    }
    if matches!(
        block.as_str(),
        "minecraft:stone" | "minecraft:redstone_block"
    ) {
        return Some(true);
    }
    declared(&block, "requires_tool")?.as_bool()
}

/// The hardness the port declares for Create's block in `state`, as the host's `destroy_speed`
/// reports it. None for other blocks (`breaking::hardness` has the vanilla ones).
pub fn hardness(state: &str) -> Option<f32> {
    declared(&crate::scenario::parse_state(state).0, "hardness")?
        .as_f64()
        .map(|h| h as f32)
}

/// The stacks the block in `state` drops, or None when its table is outside that subset. The
/// harness's own blocks have none.
pub fn drops(state: &str) -> Option<Vec<(String, u8)>> {
    let (block, props) = crate::scenario::parse_state(state);
    if block.starts_with("createfuzz:") {
        return Some(Vec::new());
    }
    let name = block.strip_prefix("create:")?;
    let path = crate::defaults::checkout()
        .join(format!("create/data/create/loot_table/blocks/{name}.json"));
    let Ok(text) = std::fs::read_to_string(path) else {
        return Some(Vec::new());
    };
    let table: Value = serde_json::from_str(&text).ok()?;
    let holds = |c: &Value| condition(c, &block, &props);
    let mut out = Vec::new();
    for pool in table["pools"].as_array().into_iter().flatten() {
        if pool["rolls"].as_f64() != Some(1.0) || pool.get("functions").is_some() {
            return None;
        }
        if !all(&pool["conditions"], &holds)? {
            continue;
        }
        let entries = pool["entries"].as_array()?;
        let [entry] = entries.as_slice() else {
            return None;
        };
        if entry["type"] != "minecraft:item"
            || entry.get("functions").is_some()
            || entry.get("conditions").is_some()
        {
            return None;
        }
        let item = entry["name"].as_str()?;
        if item != "minecraft:air" {
            out.push((item.to_owned(), 1));
        }
    }
    Some(out)
}

/// Every condition in `list` holds; None for one outside the subset.
fn all(list: &Value, holds: &impl Fn(&Value) -> Option<bool>) -> Option<bool> {
    let mut ok = true;
    for c in list.as_array().into_iter().flatten() {
        ok &= holds(c)?;
    }
    Some(ok)
}

fn condition(c: &Value, block: &str, props: &[(String, String)]) -> Option<bool> {
    match c["type"].as_str()? {
        "minecraft:survives_explosion" => Some(true),
        "minecraft:inverted" => condition(&c["term"], block, props).map(|b| !b),
        "minecraft:match_block" => {
            if c["blocks"].as_str()? != block {
                return Some(false);
            }
            Some(c["state"].as_object()?.iter().all(|(k, v)| {
                props
                    .iter()
                    .any(|(pk, pv)| pk == k && Some(pv.as_str()) == v.as_str())
            }))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn tables_give_the_block_its_item() {
        let one = |s: &str| super::drops(s).unwrap();
        assert_eq!(
            one("create:creative_motor[facing=east]"),
            [("create:creative_motor".to_owned(), 1)]
        );
        assert_eq!(
            one("create:gearbox[axis=x]"),
            [("create:vertical_gearbox".to_owned(), 1)]
        );
        assert_eq!(
            one("create:gearbox[axis=y]"),
            [("create:gearbox".to_owned(), 1)]
        );
        assert!(one("create:water_wheel_structure[facing=up]").is_empty());
        assert!(super::drops("minecraft:stone").is_none());
        assert_eq!(
            super::requires_tool("create:creative_motor[facing=up]"),
            Some(true)
        );
        assert_eq!(
            super::requires_tool("create:belt[facing=east]"),
            Some(false)
        );
        assert_eq!(
            super::hardness("create:mechanical_drill[facing=up,waterlogged=false]"),
            Some(1.5)
        );
        assert_eq!(super::hardness("minecraft:stone"), None);
    }
}
