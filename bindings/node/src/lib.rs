use napi::bindgen_prelude::*;
use napi_derive::napi;
use serde_json;
use toonify_core::{
    convert_str, decode_str, validate_str, DecoderOptions, Delimiter, EncoderOptions,
    KeyFoldingMode, PathExpansionMode, SourceFormat,
};

#[napi(object)]
pub struct ConvertOptions {
    pub format: Option<String>,
    pub delimiter: Option<String>,
    pub indent: Option<u32>,
    pub key_folding: Option<String>,
    pub flatten_depth: Option<u32>,
}

impl Default for ConvertOptions {
    fn default() -> Self {
        Self {
            format: None,
            delimiter: None,
            indent: None,
            key_folding: None,
            flatten_depth: None,
        }
    }
}

#[napi(object)]
pub struct DecodeOptions {
    pub indent: Option<u32>,
    pub expand_paths: Option<String>,
    pub loose: Option<bool>,
    pub pretty: Option<bool>,
}

impl Default for DecodeOptions {
    fn default() -> Self {
        Self {
            indent: None,
            expand_paths: None,
            loose: None,
            pretty: None,
        }
    }
}

#[napi]
pub fn convert_to_toon(input: String, options: Option<ConvertOptions>) -> napi::Result<String> {
    let opts = options.unwrap_or_default();
    let format = resolve_format(opts.format.as_deref(), &input)?;
    let delimiter = resolve_delimiter(opts.delimiter.as_deref())?;
    let indent = opts.indent.unwrap_or(2) as usize;
    let flatten_depth = opts.flatten_depth.map(|value| value as usize);

    let key_folding = match opts
        .key_folding
        .as_deref()
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        None | Some("off") => KeyFoldingMode::Off,
        Some("safe") => KeyFoldingMode::Safe { flatten_depth },
        Some(other) => {
            return Err(Error::new(
                Status::InvalidArg,
                format!("unsupported key folding mode: {other}"),
            ))
        }
    };

    let encoder_options = EncoderOptions {
        indent,
        document_delimiter: delimiter,
        key_folding,
    };

    convert_str(&input, format, encoder_options)
        .map_err(|err| Error::new(Status::GenericFailure, err.to_string()))
}

#[napi]
pub fn decode_to_json(input: String, options: Option<DecodeOptions>) -> napi::Result<String> {
    let opts = options.unwrap_or_default();
    let decoder_options = build_decoder_options(&opts)?;
    let value = decode_str(&input, decoder_options)
        .map_err(|err| Error::new(Status::GenericFailure, err.to_string()))?;
    let pretty = opts.pretty.unwrap_or(false);
    let output = if pretty {
        serde_json::to_string_pretty(&value)
            .map_err(|err| Error::new(Status::GenericFailure, err.to_string()))?
    } else {
        serde_json::to_string(&value)
            .map_err(|err| Error::new(Status::GenericFailure, err.to_string()))?
    };
    Ok(output)
}

#[napi]
pub fn validate_toon(input: String, options: Option<DecodeOptions>) -> napi::Result<()> {
    let opts = options.unwrap_or_default();
    let decoder_options = build_decoder_options(&opts)?;
    validate_str(&input, decoder_options)
        .map_err(|err| Error::new(Status::GenericFailure, err.to_string()))
}

#[napi]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn resolve_format(format: Option<&str>, sample: &str) -> napi::Result<SourceFormat> {
    match format.map(|value| value.to_ascii_lowercase()) {
        Some(value) => match value.as_str() {
            "json" => Ok(SourceFormat::Json),
            "yaml" | "yml" => Ok(SourceFormat::Yaml),
            "xml" => Ok(SourceFormat::Xml),
            "csv" => Ok(SourceFormat::Csv),
            "auto" => Ok(sniff_format(sample)),
            other => Err(Error::new(
                Status::InvalidArg,
                format!("unsupported format: {other}"),
            )),
        },
        None => Ok(sniff_format(sample)),
    }
}

fn resolve_delimiter(delimiter: Option<&str>) -> napi::Result<Delimiter> {
    Ok(match delimiter.map(|value| value.to_ascii_lowercase()) {
        Some(value) => match value.as_str() {
            "comma" => Delimiter::Comma,
            "tab" => Delimiter::Tab,
            "pipe" => Delimiter::Pipe,
            other => {
                return Err(Error::new(
                    Status::InvalidArg,
                    format!("unsupported delimiter: {other}"),
                ))
            }
        },
        None => Delimiter::Comma,
    })
}

fn sniff_format(sample: &str) -> SourceFormat {
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

fn build_decoder_options(opts: &DecodeOptions) -> napi::Result<DecoderOptions> {
    let indent = opts.indent.unwrap_or(2) as usize;
    let strict = !opts.loose.unwrap_or(false);
    let expand_paths = match opts
        .expand_paths
        .as_deref()
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        None | Some("off") => PathExpansionMode::Off,
        Some("safe") => PathExpansionMode::Safe,
        Some(other) => {
            return Err(Error::new(
                Status::InvalidArg,
                format!("unsupported expandPaths mode: {other}"),
            ))
        }
    };

    Ok(DecoderOptions {
        indent,
        strict,
        expand_paths,
    })
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
    fn node_bindings_round_trip_fixture() {
        let base = fixtures_root().join("JSONtoTOON");
        let json_input = fs::read_to_string(base.join("JSONs/td.json")).unwrap();
        let expected_toon = fs::read_to_string(base.join("TOONs_correct/td.toon")).unwrap();

        let toon = convert_to_toon(
            json_input.clone(),
            Some(ConvertOptions {
                format: Some("json".into()),
                delimiter: None,
                indent: Some(2),
                key_folding: Some("off".into()),
                flatten_depth: None,
            }),
        )
        .expect("node convert_to_toon should succeed");
        assert_eq!(toon.trim_end(), expected_toon.trim_end());

        let decoded = decode_to_json(
            expected_toon.clone(),
            Some(DecodeOptions {
                indent: Some(2),
                expand_paths: Some("off".into()),
                loose: Some(false),
                pretty: Some(false),
            }),
        )
        .expect("node decode_to_json should succeed");

        let value: Value = serde_json::from_str(&decoded).unwrap();
        let expected: Value = serde_json::from_str(&json_input).unwrap();
        assert_eq!(value, expected);
    }

    #[test]
    fn node_validator_rejects_invalid_fixture() {
        let invalid =
            fs::read_to_string(fixtures_root().join("validator/invalid_row_count.toon")).unwrap();
        assert!(validate_toon(
            invalid,
            Some(DecodeOptions {
                indent: Some(2),
                expand_paths: Some("off".into()),
                loose: Some(false),
                pretty: None,
            })
        )
        .is_err());
    }
}
