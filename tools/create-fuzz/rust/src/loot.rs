//! What a broken block drops: its loot table (`create/data/create/loot_table/blocks/<name>.json`,
//! Create's generated data), as the host rolls it for `destroyBlock(pos, true)`. Only the
//! tables of Create's blocks, and only what they use: one roll of plain item entries under
//! `survives_explosion`, `match_block`, `inverted` and `all_of` pool conditions.
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

/// The stacks the block in `state` drops. The harness's own blocks and absent tables drop
/// nothing; unreadable, malformed or unsupported tables return `None`.
pub fn drops(state: &str) -> Option<Vec<(String, u8)>> {
    let (block, props) = crate::scenario::parse_state(state);
    if block.starts_with("createfuzz:") {
        return Some(Vec::new());
    }
    let name = block.strip_prefix("create:")?;
    let path = crate::defaults::checkout()
        .join(format!("create/data/create/loot_table/blocks/{name}.json"));
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Some(Vec::new()),
        Err(_) => return None,
    };
    let table: Value = serde_json::from_str(&text).ok()?;
    table_drops(&table, &block, &props)
}

fn table_drops(
    table: &Value,
    block: &str,
    props: &[(String, String)],
) -> Option<Vec<(String, u8)>> {
    let has_modifier =
        |node: &Value| node.get("modifier").is_some() || node.get("functions").is_some();
    if has_modifier(table) {
        return None;
    }
    let pools = match table.as_object()?.get("pools") {
        None | Some(Value::Null) => &[],
        Some(pools) => pools.as_array()?.as_slice(),
    };
    let mut out = Vec::new();
    for pool in pools {
        if pool["rolls"].as_f64() != Some(1.0)
            || has_modifier(pool)
            || pool.get("conditions").is_some()
        {
            return None;
        }
        let entries = pool["entries"].as_array()?;
        let [entry] = entries.as_slice() else {
            return None;
        };
        if entry["type"] != "minecraft:item"
            || has_modifier(entry)
            || entry.get("condition").is_some()
            || entry.get("conditions").is_some()
        {
            return None;
        }
        if let Some(c) = pool.get("condition")
            && !condition(c, block, props)?
        {
            continue;
        }
        let item = entry["name"].as_str()?;
        if item != "minecraft:air" {
            out.push((item.to_owned(), 1));
        }
    }
    Some(out)
}

fn condition(c: &Value, block: &str, props: &[(String, String)]) -> Option<bool> {
    match c["type"].as_str()? {
        "minecraft:survives_explosion" => Some(true),
        "minecraft:inverted" => condition(&c["term"], block, props).map(|b| !b),
        "minecraft:all_of" => {
            for term in c["terms"].as_array()? {
                if !condition(term, block, props)? {
                    return Some(false);
                }
            }
            Some(true)
        }
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
    use serde_json::json;

    #[test]
    fn conjunction_short_circuits_in_term_order() {
        let fails =
            json!({"type": "minecraft:inverted", "term": {"type": "minecraft:survives_explosion"}});
        let unsupported = json!("create:unknown_predicate");
        let predicate = json!({"type": "minecraft:all_of", "terms": [fails, unsupported]});
        assert_eq!(
            super::condition(&predicate, "create:shaft", &[]),
            Some(false)
        );
        let predicate = json!({"type": "minecraft:all_of", "terms": [unsupported, fails]});
        assert_eq!(super::condition(&predicate, "create:shaft", &[]), None);
    }

    #[test]
    fn refuses_modifiers_and_entry_conditions() {
        let entry = json!({"type": "minecraft:item", "name": "create:shaft"});
        let pool = json!({"rolls": 1.0, "entries": [entry]});
        let table = json!({"pools": [pool]});
        let modifier = json!([{"type": "minecraft:set_count", "count": 2}]);
        let mut table_modifier = table.clone();
        table_modifier["modifier"] = modifier.clone();
        let mut pool_modifier = table.clone();
        pool_modifier["pools"][0]["modifier"] = modifier.clone();
        let mut entry_modifier = table.clone();
        entry_modifier["pools"][0]["entries"][0]["modifier"] = modifier;
        let mut entry_condition = table;
        entry_condition["pools"][0]["entries"][0]["condition"] =
            json!({"type": "minecraft:survives_explosion"});
        for unsupported in [
            table_modifier,
            pool_modifier,
            entry_modifier,
            entry_condition,
        ] {
            assert!(
                super::table_drops(&unsupported, "create:shaft", &[]).is_none(),
                "{unsupported}"
            );
        }
    }

    #[test]
    fn distinguishes_empty_tables_from_wrong_shapes() {
        for (table, expected) in [
            (json!({}), Some(Vec::new())),
            (json!({"pools": []}), Some(Vec::new())),
            (json!({"pools": null}), Some(Vec::new())),
            (json!({"pools": false}), None),
            (json!(null), None),
            (json!([]), None),
        ] {
            assert_eq!(
                super::table_drops(&table, "create:shaft", &[]),
                expected,
                "{table}"
            );
        }
    }

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
