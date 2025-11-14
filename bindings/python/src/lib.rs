use pyo3::{exceptions::PyValueError, prelude::*};
use serde_json;
use toonify_core::{
    convert_str, decode_str, validate_str, DecoderOptions, Delimiter, EncoderOptions,
    KeyFoldingMode, PathExpansionMode, SourceFormat,
};

#[pyfunction]
#[pyo3(signature = (input, *, format=None, delimiter=None, indent=2, key_folding="off", flatten_depth=None))]
fn convert_to_toon(
    input: &str,
    format: Option<&str>,
    delimiter: Option<&str>,
    indent: usize,
    key_folding: &str,
    flatten_depth: Option<usize>,
) -> PyResult<String> {
    convert_to_toon_impl(input, format, delimiter, indent, key_folding, flatten_depth)
        .map_err(PyValueError::new_err)
}

#[pyfunction]
#[pyo3(signature = (input, *, indent=2, expand_paths="off", loose=false, pretty=false))]
fn decode_to_json(
    input: &str,
    indent: usize,
    expand_paths: &str,
    loose: bool,
    pretty: bool,
) -> PyResult<String> {
    decode_to_json_impl(input, indent, expand_paths, loose, pretty).map_err(PyValueError::new_err)
}

#[pyfunction]
#[pyo3(signature = (input, *, indent=2, expand_paths="off", loose=false))]
fn validate_toon(input: &str, indent: usize, expand_paths: &str, loose: bool) -> PyResult<()> {
    validate_toon_impl(input, indent, expand_paths, loose).map_err(PyValueError::new_err)
}

#[pyfunction]
fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[pymodule]
fn toonify(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(convert_to_toon, m)?)?;
    m.add_function(wrap_pyfunction!(decode_to_json, m)?)?;
    m.add_function(wrap_pyfunction!(validate_toon, m)?)?;
    m.add_function(wrap_pyfunction!(version, m)?)?;
    m.add("__version__", version())?;
    m.add("__doc__", "Python bindings for the TOON converter")?;
    Ok(())
}

fn convert_to_toon_impl(
    input: &str,
    format: Option<&str>,
    delimiter: Option<&str>,
    indent: usize,
    key_folding: &str,
    flatten_depth: Option<usize>,
) -> Result<String, String> {
    let source_format = parse_format(format, input)?;
    let document_delimiter = parse_delimiter(delimiter)?;
    let folding = parse_key_folding(key_folding, flatten_depth)?;

    let options = EncoderOptions {
        indent,
        document_delimiter,
        key_folding: folding,
    };

    convert_str(input, source_format, options).map_err(|err| err.to_string())
}

fn decode_to_json_impl(
    input: &str,
    indent: usize,
    expand_paths: &str,
    loose: bool,
    pretty: bool,
) -> Result<String, String> {
    let options = build_decoder_options(indent, expand_paths, loose)?;
    let value = decode_str(input, options).map_err(|err| err.to_string())?;
    let json = if pretty {
        serde_json::to_string_pretty(&value).map_err(|err| err.to_string())?
    } else {
        serde_json::to_string(&value).map_err(|err| err.to_string())?
    };
    Ok(json)
}

fn validate_toon_impl(
    input: &str,
    indent: usize,
    expand_paths: &str,
    loose: bool,
) -> Result<(), String> {
    let options = build_decoder_options(indent, expand_paths, loose)?;
    validate_str(input, options).map_err(|err| err.to_string())
}

fn parse_format(value: Option<&str>, sample: &str) -> Result<SourceFormat, String> {
    match value.map(|val| val.to_ascii_lowercase()) {
        Some(v) => match v.as_str() {
            "json" => Ok(SourceFormat::Json),
            "yaml" | "yml" => Ok(SourceFormat::Yaml),
            "xml" => Ok(SourceFormat::Xml),
            "csv" => Ok(SourceFormat::Csv),
            "auto" => Ok(sniff(sample)),
            other => Err(format!("unsupported format: {other}")),
        },
        None => Ok(sniff(sample)),
    }
}

fn parse_delimiter(value: Option<&str>) -> Result<Delimiter, String> {
    Ok(match value.map(|val| val.to_ascii_lowercase()) {
        Some(v) => match v.as_str() {
            "comma" => Delimiter::Comma,
            "tab" => Delimiter::Tab,
            "pipe" => Delimiter::Pipe,
            other => return Err(format!("unsupported delimiter: {other}")),
        },
        None => Delimiter::Comma,
    })
}

fn parse_key_folding(value: &str, flatten_depth: Option<usize>) -> Result<KeyFoldingMode, String> {
    match value.to_ascii_lowercase().as_str() {
        "off" => Ok(KeyFoldingMode::Off),
        "safe" => Ok(KeyFoldingMode::Safe { flatten_depth }),
        other => Err(format!("unsupported key folding: {other}")),
    }
}

fn build_decoder_options(
    indent: usize,
    expand_paths: &str,
    loose: bool,
) -> Result<DecoderOptions, String> {
    Ok(DecoderOptions {
        indent,
        strict: !loose,
        expand_paths: parse_expand_paths(expand_paths)?,
    })
}

fn parse_expand_paths(value: &str) -> Result<PathExpansionMode, String> {
    match value.to_ascii_lowercase().as_str() {
        "off" => Ok(PathExpansionMode::Off),
        "safe" => Ok(PathExpansionMode::Safe),
        other => Err(format!("unsupported expand_paths: {other}")),
    }
}

fn sniff(sample: &str) -> SourceFormat {
    let trimmed = sample.trim_start();
    if trimmed.starts_with('<') {
        SourceFormat::Xml
    } else if trimmed.starts_with("---") || trimmed.starts_with("- ") {
        SourceFormat::Yaml
    } else if trimmed.starts_with('{') || trimmed.starts_with('[') {
        SourceFormat::Json
    } else {
        SourceFormat::Json
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::fs;
    use std::path::PathBuf;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-files")
    }

    #[test]
    fn python_helpers_round_trip_fixture() {
        let base = fixtures_root().join("JSONtoTOON");
        let json_input = fs::read_to_string(base.join("JSONs/td.json")).unwrap();
        let expected_toon = fs::read_to_string(base.join("TOONs_correct/td.toon")).unwrap();

        let rendered =
            convert_to_toon_impl(&json_input, Some("json"), None, 2, "off", None).unwrap();
        assert_eq!(rendered.trim_end(), expected_toon.trim_end());

        let decoded = decode_to_json_impl(&expected_toon, 2, "off", false, false).unwrap();
        let actual: Value = serde_json::from_str(&decoded).unwrap();
        let expected: Value = serde_json::from_str(&json_input).unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn python_validator_rejects_invalid_fixture() {
        let invalid =
            fs::read_to_string(fixtures_root().join("validator/invalid_row_count.toon")).unwrap();
        assert!(validate_toon_impl(&invalid, 2, "off", false).is_err());
    }
}
