use std::io::Read;

use crate::decoder::{decode_reader as decode_reader_internal, decode_str as decode_str_internal};
use crate::error::ToonifyError;
use crate::options::DecoderOptions;

/// Validate TOON text. Returns Ok(()) if the document is structurally sound.
pub fn validate_str(input: &str, options: DecoderOptions) -> Result<(), ToonifyError> {
    decode_str_internal(input, options)?;
    Ok(())
}

/// Validate TOON data coming from a reader.
pub fn validate_reader<R: Read>(reader: R, options: DecoderOptions) -> Result<(), ToonifyError> {
    decode_reader_internal(reader, options)?;
    Ok(())
}
