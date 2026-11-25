//! What the repository may hold under `create/assets`: generated block states, models, item
//! definitions naming models by id, connected-texture rules and atlas references, the port's
//! shader and Catnip's MIT placement indicator. Everything under Create's
//! `src/main/resources/assets/` is All Rights Reserved: textures, models, particles, sounds and
//! translations of Create's come from the player's jar through the manifest's `[[jar]]` layer,
//! never from a committed file. Where Create's jar is at hand (`$CREATE_JAR`, else the registry's
//! download cache), no committed file under `create/assets` or `create/data` may be one of the
//! jar's assets, byte for byte or as JSON (a file that holds all of a jar asset's JSON at its own
//! path, with keys added, counts as a copy).

use crate::registry::items::custom_definition;
use crate::registry::visuals::{SPINNING_TOOLS, spinner_definition};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};

fn files(dir: &Path, root: &Path, out: &mut Vec<String>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            files(&p, root, out);
        } else {
            out.push(
                p.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn only_approved_asset_types_are_committed() {
    let root = repo().join("create/assets");
    let mut all = Vec::new();
    files(&root, &root, &mut all);
    let (generated_connections, _) = crate::ct::native_assets();
    let refused: Vec<&String> = all
        .iter()
        .filter(|f| {
            if let Some(name) = f.strip_prefix("create/items/").and_then(|name| name.strip_suffix(".json")) {
                let Ok(bytes) = std::fs::read(root.join(f)) else { return true };
                let Ok(doc) = serde_json::from_slice::<Value>(&bytes) else { return true };
                if SPINNING_TOOLS.iter().any(|(item, _)| *item == name) {
                    return doc != spinner_definition(name);
                }
                if let Some(custom) = custom_definition(name) {
                    return doc != custom;
                }
                return !["block", "item"].into_iter().any(|kind| {
                    doc == serde_json::json!({ "model": { "type": "minecraft:model", "model": format!("create:{kind}/{name}") } })
                });
            }
            !(f.starts_with("create/blockstates/") && f.ends_with(".json")
                || f.starts_with("create/models/block/") && f.ends_with(".json")
                || generated_connections.get(&format!("assets/{f}")).is_some_and(|expected| std::fs::read(root.join(f)).is_ok_and(|bytes| bytes == *expected))
                || matches!(f.as_str(), "create/textures/gui/placement_indicator.png"
                    | "create/shaders/core/textured.fsh" | "minecraft/atlases/blocks.json"))
        })
        .collect();
    assert!(
        refused.is_empty(),
        "unapproved asset type; Create's protected assets come from the player's jar: {refused:?}"
    );
}

/// Whether `a` holds all of `b`: the same, or an object with each of `b`'s keys holding
/// all of `b`'s value there.
fn holds(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Object(a), Value::Object(b)) => {
            b.iter().all(|(k, v)| a.get(k).is_some_and(|w| holds(w, v)))
        }
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        _ => a == b,
    }
}

#[test]
fn no_committed_file_is_one_of_create_s_assets() {
    let jar = std::env::var_os("CREATE_JAR")
        .map(PathBuf::from)
        .or_else(|| {
            crate::jar::cache_dir()
                .ok()
                .map(|d| d.join(crate::jar::CREATE.file))
        })
        .filter(|p| p.is_file());
    let Some(jar) = jar else {
        eprintln!("no Create jar at hand ($CREATE_JAR or the registry's cache): skipped");
        return;
    };
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&jar).unwrap()).unwrap();
    let mut bytes_seen: HashSet<Vec<u8>> = HashSet::new();
    let mut json_seen: HashSet<String> = HashSet::new();
    let mut json_at: BTreeMap<String, Value> = BTreeMap::new();
    for i in 0..zip.len() {
        let mut e = zip.by_index(i).unwrap();
        let name = e.name().to_owned();
        if e.is_dir() || !name.starts_with("assets/") {
            continue;
        }
        let mut b = Vec::new();
        e.read_to_end(&mut b).unwrap();
        if name.ends_with(".json")
            && let Ok(v) = serde_json::from_slice::<Value>(&b)
        {
            json_seen.insert(v.to_string());
            json_at.insert(name, v);
        }
        bytes_seen.insert(b);
    }
    let mut copies = Vec::new();
    for top in ["create/assets", "create/data"] {
        let root = repo().join(top);
        let mut all = Vec::new();
        files(&root, &root, &mut all);
        for f in all {
            let b = std::fs::read(root.join(&f)).unwrap();
            let at = format!("{}/{f}", top.trim_start_matches("create/"));
            let json = serde_json::from_slice::<Value>(&b).ok();
            let copy = bytes_seen.contains(&b)
                || json.as_ref().is_some_and(|v| {
                    json_seen.contains(&v.to_string())
                        || json_at.get(&at).is_some_and(|j| holds(v, j))
                });
            if copy {
                copies.push(format!("{top}/{f}"));
            }
        }
    }
    assert!(
        copies.is_empty(),
        "committed files that are Create's All Rights Reserved assets: {copies:?}"
    );
}

#[test]
fn a_file_with_keys_added_still_holds_the_original() {
    let jar = serde_json::json!({"textures": ["a", "b"], "n": 1});
    let ours = serde_json::json!({"textures": ["a", "b"], "n": 1.0, "horizon:x": {}});
    assert!(holds(&ours, &jar));
    assert!(!holds(&serde_json::json!({"textures": ["a"]}), &jar));
}
