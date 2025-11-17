use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-files")
}

fn cli_cmd() -> Command {
    Command::new(assert_cmd::cargo::cargo_bin!("toonify"))
}

#[test]
fn cli_encodes_json_fixture() {
    let base = fixtures_root().join("JSONtoTOON");
    let json_path = base.join("JSONs/td.json");
    let expected_toon = fs::read_to_string(base.join("TOONs_correct/td.toon")).unwrap();

    let output = cli_cmd()
        .arg("--input")
        .arg(&json_path)
        .arg("--format")
        .arg("json")
        .output()
        .unwrap();

    assert!(output.status.success(), "CLI encode command failed");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.trim_end(), expected_toon.trim_end());
}

#[test]
fn cli_decodes_toon_fixture() {
    let base = fixtures_root().join("JSONtoTOON");
    let toon_path = base.join("TOONs_correct/td.toon");
    let expected_json: Value =
        serde_json::from_str(&fs::read_to_string(base.join("JSONs/td.json")).unwrap()).unwrap();

    let output = cli_cmd()
        .arg("--mode")
        .arg("decode")
        .arg("--input")
        .arg(&toon_path)
        .output()
        .unwrap();

    assert!(output.status.success(), "CLI decode command failed");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let actual: Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(actual, expected_json);
}

#[test]
fn cli_validator_rejects_invalid_fixture() {
    let invalid = fixtures_root().join("validator/invalid_row_count.toon");
    let output = cli_cmd()
        .arg("--mode")
        .arg("validate")
        .arg("--input")
        .arg(&invalid)
        .output()
        .expect("failed to run cli --mode validate");

    assert!(
        !output.status.success(),
        "validator should fail on invalid strict-mode fixture"
    );
}
