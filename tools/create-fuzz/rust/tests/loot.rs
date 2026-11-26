#[path = "../../../test-support/temp_dir.rs"]
mod temp_dir;

use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

#[test]
fn belt_mining_keeps_its_connector_and_reports_bad_shaft_loot() {
    let fixture = temp_dir::TempDir::new("create-fuzz-loot");
    let root = fixture.path();
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for file in [
        "tools/create-data/src/default_states.json",
        "create/data/create/sunrise/block/belt.json",
    ] {
        let target = root.join(file);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(repo.join(file), target).unwrap();
    }
    let tables = root.join("create/data/create/loot_table/blocks");
    fs::create_dir_all(&tables).unwrap();
    fs::write(
        tables.join("belt.json"),
        r#"{"pools":[{"rolls":1,"entries":[{"type":"minecraft:item","name":"create:belt_connector"}]}]}"#,
    )
    .unwrap();
    let observe = || {
        let output = Command::new(env!("CARGO_BIN_EXE_create-fuzz"))
            .arg("rust")
            .arg(repo.join("tools/create-fuzz/cases/belts-connector.json"))
            .env("CREATE_HORIZON_DIR", root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<Value>(&output.stdout).unwrap()
    };
    let shaft = tables.join("shaft.json");
    let absent = observe();
    fs::create_dir(&shaft).unwrap();
    let unreadable = observe();
    fs::remove_dir(&shaft).unwrap();
    fs::write(&shaft, "{").unwrap();
    let malformed = observe();

    assert!(absent.get("notes").is_none());
    let gap = json!(["adapter gap: loot of create:shaft at [0, 8, 0]"]);
    assert_eq!(unreadable["notes"], gap);
    assert_eq!(malformed["notes"], gap);
    for snap in [absent, unreadable, malformed] {
        assert_eq!(snap["blocks"]["0,8,0"]["state"], "minecraft:air");
        let drops = snap["drops"].as_array().unwrap();
        assert_eq!(drops.len(), 1);
        assert_eq!(
            drops[0]["stack"],
            json!({"item":"create:belt_connector", "count":1})
        );
    }
}
