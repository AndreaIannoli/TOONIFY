use once_cell::sync::OnceCell;
use tiktoken_rs::{CoreBPE, cl100k_base, o200k_base};

use crate::error::ToonifyError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TokenModel {
    Cl100k,
    O200k,
}

impl std::fmt::Display for TokenModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenModel::Cl100k => write!(f, "cl100k_base"),
            TokenModel::O200k => write!(f, "o200k_base"),
        }
    }
}

static CL100K: OnceCell<CoreBPE> = OnceCell::new();
static O200K: OnceCell<CoreBPE> = OnceCell::new();

pub fn count_tokens(text: &str, model: TokenModel) -> Result<usize, ToonifyError> {
    let tokenizer = get_tokenizer(model)?;
    Ok(tokenizer.encode_ordinary(text).len())
}

fn get_tokenizer(model: TokenModel) -> Result<&'static CoreBPE, ToonifyError> {
    match model {
        TokenModel::Cl100k => CL100K.get_or_try_init(|| {
            cl100k_base().map_err(|err| ToonifyError::tokenizer(err.to_string()))
        }),
        TokenModel::O200k => O200K.get_or_try_init(|| {
            o200k_base().map_err(|err| ToonifyError::tokenizer(err.to_string()))
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_tokens_for_simple_text() {
        let text = "Hello world!";
        let cl = count_tokens(text, TokenModel::Cl100k).unwrap();
        let o2 = count_tokens(text, TokenModel::O200k).unwrap();
        assert!(cl > 0);
        assert!(o2 > 0);
    }
}
