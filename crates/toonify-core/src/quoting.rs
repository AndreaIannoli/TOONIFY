use serde_json::Value;

use crate::options::Delimiter;

pub(crate) fn encode_key(key: &str) -> String {
    if is_identifier_key(key) {
        key.to_string()
    } else {
        format!("\"{}\"", escape(key))
    }
}

pub(crate) fn is_identifier_key(key: &str) -> bool {
    let mut chars = key.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => (),
        _ => return false,
    }

    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

pub(crate) fn is_identifier_segment(segment: &str) -> bool {
    let mut chars = segment.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => (),
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub(crate) fn encode_string(value: &str, delimiter: Option<Delimiter>) -> String {
    if needs_quotes(value, delimiter.map(|d| d.as_char())) {
        format!("\"{}\"", escape(value))
    } else {
        value.to_string()
    }
}

fn needs_quotes(value: &str, delimiter: Option<char>) -> bool {
    if value.is_empty()
        || value.trim() != value
        || value == "true"
        || value == "false"
        || value == "null"
        || is_numeric_like(value)
        || value
            .chars()
            .any(|c| matches!(c, ':' | '"' | '\\' | '[' | ']' | '{' | '}'))
        || value.chars().any(|c| matches!(c, '\n' | '\r' | '\t'))
        || value.starts_with('-')
    {
        return true;
    }

    if let Some(delim) = delimiter {
        if value.contains(delim) {
            return true;
        }
    }

    false
}

fn escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    escaped
}

fn is_numeric_like(value: &str) -> bool {
    if let Ok(Value::Number(_)) = serde_json::from_str::<Value>(value) {
        return true;
    }
    value.len() > 1 && value.starts_with('0') && value.chars().all(|c| c.is_ascii_digit())
}
