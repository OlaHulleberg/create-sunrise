//! Item definitions (26.3's `assets/<namespace>/items/<id>.json`, which 1.21.1 did not use):
//! each names models by id only; the models stay in the player's jar.

use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

/// The item models of Create's `CustomRenderedItemModelRenderer`s, as 26.3 item definitions: the
/// item's own model with the partial models its renderer draws over it, and the cardboard
/// sword's held model outside the GUI. The wand's bits stand still. None for an item drawn by
/// its plain model.
fn custom_rendered(name: &str) -> Option<Value> {
    let model =
        |m: &str| json!({ "type": "minecraft:model", "model": format!("create:item/{name}/{m}") });
    let parts: &[&str] = match name {
        // `SymmetryWandItemRenderer`: CORE, CORE_GLOW, BITS.
        "wand_of_symmetry" => &["item", "core", "core_glow", "bits"],
        // `ExtendoGripItemRenderer`: its arms, hand and cog posed by the mod's special model
        // renderer (`create:extendo_grip`, `glue/equipment.rs`).
        // The glue's `create:extendo_grip_pose` node picks one by where it is drawn and the
        // time (its per-stack callback): at rest away from a first-person hand, else as the swing
        // left it; the cog turned to a 1/10 of its 45° repeat; the hand pointing, punching or
        // holding.
        "extendo_grip" => {
            let special = |animation: f64, cog: f64, hand: &str| {
                json!({
                    "type": "minecraft:special",
                    "base": "create:item/extendo_grip",
                    "model": {
                        "type": "create:extendo_grip",
                        "animation": animation,
                        "cog": cog,
                        "hand": hand,
                    },
                })
            };
            let mut models = Map::new();
            for c in 0..GRIP_COG_STEPS {
                let cog = c as f64 * 45.0 / GRIP_COG_STEPS as f64;
                models.insert(format!("rest_{c}"), special(0.25, cog, "pointing"));
                for a in 0..GRIP_SWING_STEPS {
                    let animation = a as f64 / (GRIP_SWING_STEPS - 1) as f64;
                    for hand in ["punching", "holding"] {
                        models.insert(
                            format!("hand_{a}_{c}_{hand}"),
                            special(animation, cog, hand),
                        );
                    }
                }
            }
            return Some(json!({ "type": "create:extendo_grip_pose", "models": models }));
        }
        "cardboard_sword" => {
            return Some(json!({
                "type": "minecraft:select",
                "property": "minecraft:display_context",
                "cases": [{ "when": "gui", "model": model("item") }],
                "fallback": model("item_in_hand"),
            }));
        }
        _ => return None,
    };
    Some(json!({
        "type": "minecraft:composite",
        "models": parts.iter().map(|part| model(part)).collect::<Vec<_>>(),
    }))
}

/// The extendo grip's cog angles within its 45° repeat, and its swing's steps (the glue's
/// `GRIP_COG_STEPS`, `GRIP_SWING_STEPS`).
const GRIP_COG_STEPS: usize = 10;
const GRIP_SWING_STEPS: usize = 8;

pub(crate) struct ItemModel {
    target: String,
    custom: Option<Value>,
}

impl ItemModel {
    pub(crate) fn definition(&self) -> Value {
        self.custom.clone().unwrap_or_else(|| json!({ "model": { "type": "minecraft:model", "model": format!("create:{}", self.target) } }))
    }
}

/// The definition of an item drawn by more than its plain model, if it is one.
pub(crate) fn custom_definition(name: &str) -> Option<Value> {
    custom_rendered(name).map(|model| json!({ "model": model }))
}

/// 26.3 looks up `assets/<namespace>/items/<id>.json`, which 1.21.1 did not use.
/// A world fluid adds a bucket item; the hidden partial-block declarations add no items.
pub(crate) fn item_models(
    files: &BTreeMap<String, Vec<u8>>,
    has_asset: impl Fn(&str) -> bool,
) -> BTreeMap<String, ItemModel> {
    let mut models = BTreeMap::new();
    for path in files.keys() {
        let block_or_item = path
            .strip_prefix("data/create/sunrise/block/")
            .or_else(|| path.strip_prefix("data/create/sunrise/item/"));
        let fluid = path.strip_prefix("data/create/sunrise/fluid/");
        let Some(name) = block_or_item
            .or(fluid)
            .and_then(|n| n.strip_suffix(".json"))
        else {
            continue;
        };
        if name.starts_with("part_") {
            continue;
        }
        let name = if fluid.is_some() {
            format!("{name}_bucket")
        } else {
            name.to_owned()
        };
        let kind =
            if fluid.is_some() || has_asset(&format!("assets/create/models/item/{name}.json")) {
                "item"
            } else {
                "block"
            };
        models.insert(
            format!("assets/create/items/{name}.json"),
            ItemModel {
                target: format!("{kind}/{name}"),
                custom: custom_definition(&name),
            },
        );
    }
    models
}

/// The model ids a definition names (`model` and `base` strings), in order, once each.
pub(crate) fn named_models(definition: &Value) -> Vec<String> {
    fn walk(v: &Value, out: &mut Vec<String>) {
        match v {
            Value::Object(m) => {
                for (k, v) in m {
                    match v {
                        Value::String(id) if k == "model" || k == "base" => {
                            if !out.contains(id) {
                                out.push(id.clone());
                            }
                        }
                        _ => walk(v, out),
                    }
                }
            }
            Value::Array(a) => a.iter().for_each(|v| walk(v, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(definition, &mut out);
    out
}

/// Where the jar holds the model `id` (`create:item/wrench`).
pub(crate) fn model_path(id: &str) -> String {
    let (ns, path) = id.split_once(':').unwrap_or(("minecraft", id));
    format!("assets/{ns}/models/{path}.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_definitions_name_only_model_ids() {
        let wand = custom_definition("wand_of_symmetry").unwrap();
        assert_eq!(
            named_models(&wand),
            ["item", "core", "core_glow", "bits"]
                .map(|p| format!("create:item/wand_of_symmetry/{p}"))
        );
        let grip = custom_definition("extendo_grip").unwrap();
        assert_eq!(named_models(&grip), ["create:item/extendo_grip"]);
        assert_eq!(
            model_path("create:item/extendo_grip"),
            "assets/create/models/item/extendo_grip.json"
        );
        assert!(custom_definition("wrench").is_none());
    }
}
