use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use toonify_core::{
    convert_str, decode_str, validate_str, DecoderOptions, EncoderOptions, SourceFormat,
};

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-files")
}

#[test]
fn json_fixtures_round_trip() {
    let base = fixtures_root().join("JSONtoTOON");
    let json_dir = base.join("JSONs");
    let toon_dir = base.join("TOONs_correct");

    for entry in fs::read_dir(&json_dir).expect("fixture dir exists") {
        let entry = entry.expect("read_dir entry");
        if !entry.file_type().expect("file type").is_file() {
            continue;
        }
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }

        let stem = path
            .file_stem()
            .and_then(|name| name.to_str())
            .expect("fixture stem");

        let json_input = fs::read_to_string(&path).expect("read json fixture");
        let expected_toon =
            fs::read_to_string(toon_dir.join(format!("{stem}.toon"))).expect("expected toon");

        let rendered = convert_str(&json_input, SourceFormat::Json, EncoderOptions::default())
            .expect("conversion succeeds");

        assert_eq!(
            rendered.trim_end(),
            expected_toon.trim_end(),
            "unexpected TOON output for fixture {stem}"
        );

        let decoded =
            decode_str(&expected_toon, DecoderOptions::default()).expect("decode succeeds");
        let expected_json: Value = serde_json::from_str(&json_input).expect("parse json");
        assert_eq!(decoded, expected_json, "round-trip mismatch for {stem}");
    }
}

#[test]
fn validator_rejects_invalid_fixture() {
    let path = fixtures_root().join("validator/invalid_row_count.toon");
    let doc = fs::read_to_string(path).expect("read validator fixture");
    assert!(validate_str(&doc, DecoderOptions::default()).is_err());
}
