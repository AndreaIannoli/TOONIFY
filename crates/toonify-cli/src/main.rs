use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{ArgAction, CommandFactory, Parser, ValueEnum};
use serde_json;
use toonify_core::{
    convert_str, decode_str, validate_str, DecoderOptions, Delimiter, EncoderOptions,
    KeyFoldingMode, PathExpansionMode, SourceFormat,
};

const LOGO: &str = r#"┌────────────────────────────┐
│░▀█▀░█▀█░█▀█░█▀█░▀█▀░█▀▀░█░█│
│░░█░░█░█░█░█░█░█░░█░░█▀▀░░█░│
│░░▀░░▀▀▀░▀▀▀░▀░▀░▀▀▀░▀░░░░▀░│
└────────────────────────────┘"#;

#[derive(Parser, Debug)]
#[command(
    name = "toonify",
    about = "Convert structured data into TOON",
    version,
    before_help = LOGO
)]
struct Cli {
    /// Input file path (defaults to STDIN)
    #[arg(short, long)]
    input: Option<PathBuf>,

    /// Output file path (defaults to STDOUT)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Select the input parser. Auto uses file extension/heuristics.
    #[arg(short = 'f', long, value_enum, default_value_t = FormatArg::Auto)]
    format: FormatArg,

    /// Document delimiter that drives quoting rules.
    #[arg(long, value_enum, default_value_t = DelimiterArg::Comma)]
    delimiter: DelimiterArg,

    /// Enable safe key folding for dotted paths.
    #[arg(long, value_enum, default_value_t = KeyFoldingArg::Off)]
    key_folding: KeyFoldingArg,

    /// Limit folded segments (only meaningful when key folding = safe).
    #[arg(long)]
    flatten_depth: Option<usize>,

    /// Spaces per indentation level.
    #[arg(long, default_value_t = 2)]
    indent: usize,

    /// Run mode: encode (default), decode TOON -> JSON, or validate TOON structure.
    #[arg(long, value_enum, default_value_t = ModeArg::Encode)]
    mode: ModeArg,

    /// Expected indentation width when decoding/validating TOON.
    #[arg(long = "decoder-indent", default_value_t = 2)]
    decoder_indent: usize,

    /// Path expansion behavior when decoding.
    #[arg(long = "expand-paths", value_enum, default_value_t = PathExpandArg::Off)]
    expand_paths: PathExpandArg,

    /// Disable strict-mode validation when decoding/validating.
    #[arg(long, action = ArgAction::SetTrue)]
    loose: bool,

    /// Pretty-print JSON when decoding.
    #[arg(long, action = ArgAction::SetTrue)]
    pretty_json: bool,
}

fn main() -> Result<()> {
    maybe_print_logo_version();
    let cli = Cli::parse();
    let mut input = String::new();

    if let Some(path) = &cli.input {
        input = fs::read_to_string(path)
            .with_context(|| format!("failed to read input file {}", path.display()))?;
    } else {
        io::stdin()
            .read_to_string(&mut input)
            .context("failed to read from STDIN")?;
    }

    match cli.mode {
        ModeArg::Encode => {
            if matches!(cli.key_folding, KeyFoldingArg::Off) && cli.flatten_depth.is_some() {
                eprintln!("warning: --flatten-depth is ignored unless --key-folding safe is set");
            }

            let format = cli.format.resolve(cli.input.as_deref(), &input);
            let toon =
                convert_str(&input, format, cli.build_options()).context("conversion failed")?;
            cli.emit(&toon)?;
        }
        ModeArg::Decode => {
            let value = decode_str(&input, cli.build_decoder_options()).context("decode failed")?;
            let output = if cli.pretty_json {
                serde_json::to_string_pretty(&value)?
            } else {
                serde_json::to_string(&value)?
            };
            cli.emit(&output)?;
        }
        ModeArg::Validate => {
            validate_str(&input, cli.build_decoder_options()).context("validation failed")?;
            let message = "TOON document is valid\n";
            cli.emit(message)?;
        }
    }

    Ok(())
}

fn maybe_print_logo_version() {
    if std::env::args().any(|arg| arg == "--version" || arg == "-V") {
        println!("{LOGO}");
        println!("{}", Cli::command().render_version());
        std::process::exit(0);
    }
}

impl Cli {
    fn build_options(&self) -> EncoderOptions {
        let key_folding = match self.key_folding {
            KeyFoldingArg::Off => KeyFoldingMode::Off,
            KeyFoldingArg::Safe => KeyFoldingMode::Safe {
                flatten_depth: self.flatten_depth,
            },
        };

        EncoderOptions {
            indent: self.indent,
            document_delimiter: self.delimiter.to_core(),
            key_folding,
        }
    }

    fn build_decoder_options(&self) -> DecoderOptions {
        DecoderOptions {
            indent: self.decoder_indent,
            strict: !self.loose,
            expand_paths: self.expand_paths.to_core(),
        }
    }

    fn emit(&self, data: &str) -> Result<()> {
        if let Some(path) = &self.output {
            fs::write(path, data)
                .with_context(|| format!("failed to write output to {}", path.display()))?;
        } else {
            print!("{data}");
        }
        Ok(())
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum FormatArg {
    Auto,
    Json,
    Yaml,
    Xml,
    Csv,
}

impl FormatArg {
    fn resolve(self, path: Option<&Path>, sample: &str) -> SourceFormat {
        match self {
            FormatArg::Auto => detect_from_path(path)
                .or_else(|| detect_from_content(sample))
                .unwrap_or(SourceFormat::Json),
            FormatArg::Json => SourceFormat::Json,
            FormatArg::Yaml => SourceFormat::Yaml,
            FormatArg::Xml => SourceFormat::Xml,
            FormatArg::Csv => SourceFormat::Csv,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum DelimiterArg {
    Comma,
    Tab,
    Pipe,
}

impl DelimiterArg {
    fn to_core(self) -> Delimiter {
        match self {
            DelimiterArg::Comma => Delimiter::Comma,
            DelimiterArg::Tab => Delimiter::Tab,
            DelimiterArg::Pipe => Delimiter::Pipe,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum KeyFoldingArg {
    Off,
    Safe,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum ModeArg {
    Encode,
    Decode,
    Validate,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum PathExpandArg {
    Off,
    Safe,
}

impl PathExpandArg {
    fn to_core(self) -> PathExpansionMode {
        match self {
            PathExpandArg::Off => PathExpansionMode::Off,
            PathExpandArg::Safe => PathExpansionMode::Safe,
        }
    }
}

fn detect_from_path(path: Option<&Path>) -> Option<SourceFormat> {
    let ext = path?.extension()?.to_string_lossy().to_ascii_lowercase();
    match ext.as_str() {
        "json" => Some(SourceFormat::Json),
        "yaml" | "yml" => Some(SourceFormat::Yaml),
        "xml" => Some(SourceFormat::Xml),
        "csv" => Some(SourceFormat::Csv),
        _ => None,
    }
}

fn detect_from_content(sample: &str) -> Option<SourceFormat> {
    let trimmed = sample.trim_start();
    if trimmed.starts_with('<') {
        Some(SourceFormat::Xml)
    } else if trimmed.starts_with("---") || trimmed.starts_with("- ") {
        Some(SourceFormat::Yaml)
    } else if trimmed.starts_with('{') || trimmed.starts_with('[') {
        Some(SourceFormat::Json)
    } else {
        None
    }
}
