/// Sets the delimiter used for document-level quoting decisions and the default
/// delimiter emitted by array headers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Delimiter {
    Comma,
    Tab,
    Pipe,
}

impl Delimiter {
    pub(crate) fn as_char(self) -> char {
        match self {
            Delimiter::Comma => ',',
            Delimiter::Tab => '\t',
            Delimiter::Pipe => '|',
        }
    }

    pub(crate) fn bracket_suffix(self) -> &'static str {
        match self {
            Delimiter::Comma => "",
            Delimiter::Tab => "\t",
            Delimiter::Pipe => "|",
        }
    }

    pub(crate) fn separator(self) -> &'static str {
        match self {
            Delimiter::Comma => ",",
            Delimiter::Tab => "\t",
            Delimiter::Pipe => "|",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyFoldingMode {
    Off,
    Safe { flatten_depth: Option<usize> },
}

impl KeyFoldingMode {
    pub(crate) fn flatten_depth(self) -> Option<usize> {
        match self {
            KeyFoldingMode::Off => None,
            KeyFoldingMode::Safe { flatten_depth } => flatten_depth.or(Some(usize::MAX)),
        }
    }

    pub(crate) fn is_enabled(self) -> bool {
        !matches!(self, KeyFoldingMode::Off)
    }
}

#[derive(Clone, Debug)]
pub struct EncoderOptions {
    pub indent: usize,
    pub document_delimiter: Delimiter,
    pub key_folding: KeyFoldingMode,
}

impl Default for EncoderOptions {
    fn default() -> Self {
        Self {
            indent: 2,
            document_delimiter: Delimiter::Comma,
            key_folding: KeyFoldingMode::Off,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathExpansionMode {
    Off,
    Safe,
}

#[derive(Clone, Debug)]
pub struct DecoderOptions {
    pub indent: usize,
    pub strict: bool,
    pub expand_paths: PathExpansionMode,
}

impl Default for DecoderOptions {
    fn default() -> Self {
        Self {
            indent: 2,
            strict: true,
            expand_paths: PathExpansionMode::Off,
        }
    }
}
