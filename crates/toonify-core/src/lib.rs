mod decoder;
mod encoder;
mod error;
mod input;
mod options;
mod quoting;
mod tokens;
mod validator;

pub use crate::decoder::{decode_reader, decode_str};
pub use crate::encoder::encode_value;
pub use crate::error::ToonifyError;
pub use crate::input::{load_from_reader, load_from_str, SourceFormat};
pub use crate::options::{
    DecoderOptions, Delimiter, EncoderOptions, KeyFoldingMode, PathExpansionMode,
};
pub use crate::tokens::{count_tokens, TokenModel};
pub use crate::validator::{validate_reader, validate_str};

/// Convert the provided string in the given `SourceFormat` into TOON.
pub fn convert_str(
    input: &str,
    format: SourceFormat,
    options: EncoderOptions,
) -> Result<String, ToonifyError> {
    let value = load_from_str(input, format)?;
    encode_value(&value, &options)
}

/// Convert readable input (JSON/YAML/XML/CSV) into TOON.
pub fn convert_reader<R: std::io::Read>(
    mut reader: R,
    format: SourceFormat,
    options: EncoderOptions,
) -> Result<String, ToonifyError> {
    let mut buf = String::new();
    std::io::Read::read_to_string(&mut reader, &mut buf)?;
    convert_str(&buf, format, options)
}
