use std::io::Read;
use std::str::FromStr;

use serde_json::{Map, Number, Value};

use crate::error::ToonifyError;
use crate::options::{DecoderOptions, Delimiter, PathExpansionMode};
use crate::quoting::is_identifier_segment;

/// Decode TOON text into a serde_json::Value.
pub fn decode_str(input: &str, options: DecoderOptions) -> Result<Value, ToonifyError> {
    let mut decoder = Decoder::new(input, options)?;
    let mut value = decoder.parse_root()?;

    if matches!(decoder.options.expand_paths, PathExpansionMode::Safe) {
        value = expand_paths(value, decoder.options.strict)?;
    }

    Ok(value)
}

/// Decode TOON from any reader.
pub fn decode_reader<R: Read>(
    mut reader: R,
    options: DecoderOptions,
) -> Result<Value, ToonifyError> {
    let mut buf = String::new();
    reader.read_to_string(&mut buf)?;
    decode_str(&buf, options)
}

struct Decoder {
    lines: Vec<Line>,
    index: usize,
    options: DecoderOptions,
}

#[derive(Clone, Debug)]
struct Line {
    depth: usize,
    text: String,
    number: usize,
}

impl Decoder {
    fn new(input: &str, options: DecoderOptions) -> Result<Self, ToonifyError> {
        let mut lines = Vec::new();
        for (idx, raw) in input.lines().enumerate() {
            let line_number = idx + 1;
            if raw.trim().is_empty() {
                continue;
            }

            let mut indent_chars = 0usize;
            for ch in raw.chars() {
                match ch {
                    ' ' => indent_chars += 1,
                    '\t' => {
                        return Err(ToonifyError::decoding(format!(
                            "line {line_number}: tabs are not allowed for indentation"
                        )))
                    }
                    _ => break,
                }
            }

            if indent_chars % options.indent != 0 {
                return Err(ToonifyError::decoding(format!(
                    "line {line_number}: indentation must be a multiple of {} spaces",
                    options.indent
                )));
            }

            let depth = indent_chars / options.indent;
            let text = raw[indent_chars..].trim_end();
            if text.is_empty() {
                continue;
            }

            lines.push(Line {
                depth,
                text: text.to_string(),
                number: line_number,
            });
        }

        Ok(Self {
            lines,
            index: 0,
            options,
        })
    }

    fn parse_root(&mut self) -> Result<Value, ToonifyError> {
        if self.lines.is_empty() {
            return Ok(Value::Object(Map::new()));
        }

        if self.lines[0].text.starts_with('[') {
            let header = self
                .parse_header_for_line(&self.lines[0], false)?
                .ok_or_else(|| {
                    ToonifyError::decoding(format!(
                        "line {}: expected array header",
                        self.lines[0].number
                    ))
                })?;
            self.index += 1;
            return self.consume_array(header, 0);
        }

        if !self.lines[0].text.contains(':') {
            let value = parse_primitive_token(self.lines[0].text.trim()).map_err(|err| {
                ToonifyError::decoding(format!("line {}: {err}", self.lines[0].number))
            })?;
            self.index = self.lines.len();
            return Ok(value);
        }

        let object = self.parse_object(0)?;
        Ok(Value::Object(object))
    }

    fn parse_object(&mut self, depth: usize) -> Result<Map<String, Value>, ToonifyError> {
        let mut map = Map::new();
        while let Some(line) = self.peek_line().cloned() {
            if line.depth != depth {
                break;
            }

            if let Some(header) = self.try_parse_header(&line, true)? {
                self.index += 1;
                let key = header.key.clone().ok_or_else(|| {
                    ToonifyError::decoding(format!(
                        "line {}: array header requires a key",
                        line.number
                    ))
                })?;
                let value = self.consume_array(header, depth)?;
                map.insert(key, value);
                continue;
            }

            self.consume_field(&mut map, depth)?;
        }
        Ok(map)
    }

    fn consume_field(
        &mut self,
        map: &mut Map<String, Value>,
        depth: usize,
    ) -> Result<(), ToonifyError> {
        let line = self
            .peek_line()
            .cloned()
            .ok_or_else(|| ToonifyError::decoding("unexpected end of document"))?;

        if let Some(header) = self.parse_header_for_line(&line, true)? {
            self.index += 1;
            let key = header.key.clone().ok_or_else(|| {
                ToonifyError::decoding(format!("line {}: array header requires a key", line.number))
            })?;
            let value = self.consume_array(header, depth)?;
            map.insert(key, value);
            return Ok(());
        }

        let (raw_key, rest) = split_key_value(&line.text).ok_or_else(|| {
            ToonifyError::decoding(format!("line {}: expected `key: value`", line.number))
        })?;
        let key = parse_key_token(raw_key)
            .map_err(|err| ToonifyError::decoding(format!("line {}: {err}", line.number)))?;

        self.index += 1;

        if rest.trim().is_empty() {
            // Nested structure
            if let Some(next) = self.peek_line() {
                if next.depth <= depth {
                    map.insert(key, Value::Object(Map::new()));
                    return Ok(());
                }
            } else {
                map.insert(key, Value::Object(Map::new()));
                return Ok(());
            }

            let value = self.parse_value_block(depth + 1)?;
            map.insert(key, value);
            return Ok(());
        }

        let value = parse_primitive_token(rest.trim())
            .map_err(|err| ToonifyError::decoding(format!("line {}: {err}", line.number)))?;
        map.insert(key, value);
        Ok(())
    }

    fn parse_value_block(&mut self, depth: usize) -> Result<Value, ToonifyError> {
        if let Some(line) = self.peek_line() {
            if line.depth != depth {
                return Ok(Value::Object(Map::new()));
            }

            if line.text.starts_with('[') {
                let header = self.parse_header_for_line(line, false)?.ok_or_else(|| {
                    ToonifyError::decoding(format!("line {}: expected array header", line.number))
                })?;
                self.index += 1;
                return self.consume_array(header, depth - 1);
            }

            if split_key_value(&line.text).is_some() {
                let object = self.parse_object(depth)?;
                return Ok(Value::Object(object));
            }

            let value = parse_primitive_token(line.text.trim())
                .map_err(|err| ToonifyError::decoding(format!("line {}: {err}", line.number)))?;
            self.index += 1;
            return Ok(value);
        }

        Ok(Value::Null)
    }

    fn try_parse_header(
        &self,
        line: &Line,
        expect_key: bool,
    ) -> Result<Option<ArrayHeader>, ToonifyError> {
        if !line.text.contains('[') {
            return Ok(None);
        }

        if let Some(header) = self.parse_header_for_line(line, expect_key)? {
            return Ok(Some(header));
        }

        Ok(None)
    }

    fn parse_header_for_line(
        &self,
        line: &Line,
        expect_key: bool,
    ) -> Result<Option<ArrayHeader>, ToonifyError> {
        parse_header(&line.text, expect_key, line.number)
    }

    fn consume_array(
        &mut self,
        header: ArrayHeader,
        container_depth: usize,
    ) -> Result<Value, ToonifyError> {
        if let Some(inline) = header
            .inline_values
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            return self.parse_inline_array(header.len, header.delimiter, inline, header.line);
        }

        if header.fields.is_some() {
            return self.parse_tabular_array(header, container_depth);
        }

        self.parse_list_array(header, container_depth)
    }

    fn parse_inline_array(
        &self,
        len: usize,
        delimiter: Delimiter,
        values: &str,
        line: usize,
    ) -> Result<Value, ToonifyError> {
        let cells = split_delimited(values, delimiter)?;
        if self.options.strict && cells.len() != len {
            return Err(ToonifyError::decoding(format!(
                "line {line}: expected {len} values but found {}",
                cells.len()
            )));
        }

        let mut out = Vec::with_capacity(cells.len());
        for cell in cells {
            let value = parse_primitive_token(cell.trim())
                .map_err(|err| ToonifyError::decoding(format!("line {line}: {err}")))?;
            out.push(value);
        }
        Ok(Value::Array(out))
    }

    fn parse_tabular_array(
        &mut self,
        header: ArrayHeader,
        container_depth: usize,
    ) -> Result<Value, ToonifyError> {
        let fields = header.fields.clone().unwrap_or_default();
        let row_depth = container_depth + 1;
        let mut rows = Vec::new();

        while let Some(line) = self.peek_line().cloned() {
            if line.depth != row_depth {
                break;
            }

            if !is_tabular_row_line(&line.text, header.delimiter) {
                break;
            }

            let cells = split_delimited(&line.text, header.delimiter)?;
            if self.options.strict && cells.len() != fields.len() {
                return Err(ToonifyError::decoding(format!(
                    "line {}: expected {} cells but found {}",
                    line.number,
                    fields.len(),
                    cells.len()
                )));
            }

            let mut map = Map::new();
            for (idx, field) in fields.iter().enumerate() {
                let cell = cells.get(idx).map(|s| s.trim()).unwrap_or("");
                let value = parse_primitive_token(cell).map_err(|err| {
                    ToonifyError::decoding(format!("line {}: {err}", line.number))
                })?;
                map.insert(field.clone(), value);
            }

            rows.push(Value::Object(map));
            self.index += 1;
        }

        if self.options.strict && rows.len() != header.len {
            return Err(ToonifyError::decoding(format!(
                "line {}: expected {} rows but found {}",
                header.line,
                header.len,
                rows.len()
            )));
        }

        Ok(Value::Array(rows))
    }

    fn parse_list_array(
        &mut self,
        header: ArrayHeader,
        container_depth: usize,
    ) -> Result<Value, ToonifyError> {
        let row_depth = container_depth + 1;
        let mut items = Vec::new();

        while let Some(line) = self.peek_line().cloned() {
            if line.depth != row_depth {
                break;
            }

            if !line.text.starts_with("- ") {
                return Err(ToonifyError::decoding(format!(
                    "line {}: expected '-' to start list item",
                    line.number
                )));
            }

            let remainder = line.text[2..].trim();
            self.index += 1;

            let value = if remainder.is_empty() {
                let object = self.parse_object(row_depth + 1)?;
                Value::Object(object)
            } else if let Some(sub_header) = parse_header(remainder, false, line.number)? {
                let key = sub_header.key.clone();
                let value = self.consume_nested_header(sub_header, row_depth)?;
                if let Some(key) = key {
                    let mut map = Map::new();
                    map.insert(key, value);
                    while let Some(next) = self.peek_line() {
                        if next.depth != row_depth + 1 {
                            break;
                        }
                        self.consume_field(&mut map, row_depth + 1)?;
                    }
                    Value::Object(map)
                } else {
                    value
                }
            } else if remainder.contains(':') {
                self.parse_inline_object_in_list(remainder, row_depth, line.number)?
            } else {
                parse_primitive_token(remainder)
                    .map_err(|err| ToonifyError::decoding(format!("line {}: {err}", line.number)))?
            };

            items.push(value);
        }

        if self.options.strict && items.len() != header.len {
            return Err(ToonifyError::decoding(format!(
                "line {}: expected {} list items but found {}",
                header.line,
                header.len,
                items.len()
            )));
        }

        Ok(Value::Array(items))
    }

    fn consume_nested_header(
        &mut self,
        mut header: ArrayHeader,
        row_depth: usize,
    ) -> Result<Value, ToonifyError> {
        // The header was parsed from an inline string, so do not advance index again.
        header.key = None;
        self.consume_array(header, row_depth)
    }

    fn parse_inline_object_in_list(
        &mut self,
        inline: &str,
        row_depth: usize,
        line_number: usize,
    ) -> Result<Value, ToonifyError> {
        let (raw_key, rest) = split_key_value(inline).ok_or_else(|| {
            ToonifyError::decoding(format!("line {line_number}: invalid list object syntax"))
        })?;
        let key = parse_key_token(raw_key)
            .map_err(|err| ToonifyError::decoding(format!("line {line_number}: {err}")))?;

        let mut map = Map::new();
        if rest.trim().is_empty() {
            let value = self.parse_value_block(row_depth + 2)?;
            map.insert(key, value);
        } else {
            let value = parse_primitive_token(rest.trim())
                .map_err(|err| ToonifyError::decoding(format!("line {line_number}: {err}")))?;
            map.insert(key, value);
        }

        while let Some(next) = self.peek_line() {
            if next.depth != row_depth + 1 {
                break;
            }
            self.consume_field(&mut map, row_depth + 1)?;
        }

        Ok(Value::Object(map))
    }

    fn peek_line(&self) -> Option<&Line> {
        self.lines.get(self.index)
    }
}

#[derive(Clone, Debug)]
struct ArrayHeader {
    key: Option<String>,
    len: usize,
    delimiter: Delimiter,
    fields: Option<Vec<String>>,
    inline_values: Option<String>,
    line: usize,
}

fn parse_header(
    text: &str,
    expect_key: bool,
    line: usize,
) -> Result<Option<ArrayHeader>, ToonifyError> {
    let colon_idx = match text.find(':') {
        Some(idx) => idx,
        None => return Ok(None),
    };

    let before = text[..colon_idx].trim_end();
    let after = text[colon_idx + 1..].trim_start();

    if !before.contains('[') {
        return Ok(None);
    }

    let bracket_idx = before
        .rfind('[')
        .ok_or_else(|| ToonifyError::decoding(format!("line {line}: malformed array header")))?;

    let (raw_key, bracket_part) = if bracket_idx == 0 {
        (None, before)
    } else {
        let key_text = before[..bracket_idx].trim_end();
        let key = parse_key_token(key_text)
            .map_err(|err| ToonifyError::decoding(format!("line {line}: {err}")))?;
        (Some(key), &before[bracket_idx..])
    };

    if expect_key && raw_key.is_none() {
        return Err(ToonifyError::decoding(format!(
            "line {line}: array header must include a key"
        )));
    }

    let closing = bracket_part
        .find(']')
        .ok_or_else(|| ToonifyError::decoding(format!("line {line}: missing closing ']'")))?;

    let mut bracket_inner = bracket_part[1..closing].trim();
    let delimiter = if bracket_inner.ends_with('|') {
        bracket_inner = &bracket_inner[..bracket_inner.len() - 1];
        Delimiter::Pipe
    } else if bracket_inner.ends_with('\t') {
        bracket_inner = &bracket_inner[..bracket_inner.len() - 1];
        Delimiter::Tab
    } else {
        Delimiter::Comma
    };

    let len: usize = bracket_inner
        .parse()
        .map_err(|_| ToonifyError::decoding(format!("line {line}: invalid array length")))?;

    let mut remainder = bracket_part[closing + 1..].trim_start();
    let fields = if remainder.starts_with('{') {
        let closing_brace = remainder.find('}').ok_or_else(|| {
            ToonifyError::decoding(format!("line {line}: missing '}}' in field list"))
        })?;
        let field_segment = &remainder[1..closing_brace];
        let list = parse_field_list(field_segment, delimiter)?;
        remainder = remainder[closing_brace + 1..].trim_start();
        Some(list)
    } else {
        None
    };

    if !remainder.is_empty() {
        return Err(ToonifyError::decoding(format!(
            "line {line}: unexpected content after array header"
        )));
    }

    Ok(Some(ArrayHeader {
        key: raw_key,
        len,
        delimiter,
        fields,
        inline_values: if after.is_empty() {
            None
        } else {
            Some(after.to_string())
        },
        line,
    }))
}

fn parse_field_list(segment: &str, delimiter: Delimiter) -> Result<Vec<String>, ToonifyError> {
    let mut fields = Vec::new();
    for raw in split_delimited(segment, delimiter)? {
        let key = parse_key_token(raw.trim())
            .map_err(|err| ToonifyError::decoding(format!("invalid field name: {err}")))?;
        fields.push(key);
    }
    Ok(fields)
}

fn split_key_value(text: &str) -> Option<(&str, &str)> {
    let mut in_quotes = false;
    let mut escaped = false;
    for (idx, ch) in text.char_indices() {
        match ch {
            '"' if !escaped => in_quotes = !in_quotes,
            '\\' if in_quotes => {
                escaped = !escaped;
                continue;
            }
            ':' if !in_quotes => {
                let key = text[..idx].trim_end();
                let value = text[idx + 1..].trim_start();
                return Some((key, value));
            }
            _ => {}
        }
        escaped = false;
    }
    None
}

fn parse_key_token(raw: &str) -> Result<String, String> {
    if raw.starts_with('"') {
        return parse_quoted_string(raw);
    }
    if raw.is_empty() {
        return Err("key cannot be empty".into());
    }
    Ok(raw.to_string())
}

fn parse_quoted_string(raw: &str) -> Result<String, String> {
    if !raw.ends_with('"') {
        return Err("unterminated string".into());
    }
    let inner = &raw[1..raw.len() - 1];
    let mut chars = inner.chars();
    let mut out = String::with_capacity(inner.len());
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            let escaped = chars
                .next()
                .ok_or_else(|| "unterminated escape".to_string())?;
            match escaped {
                '\\' => out.push('\\'),
                '"' => out.push('"'),
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                other => {
                    return Err(format!("unsupported escape \\{other}"));
                }
            }
        } else {
            out.push(ch);
        }
    }
    Ok(out)
}

fn parse_primitive_token(token: &str) -> Result<Value, String> {
    if token.starts_with('"') {
        return parse_quoted_string(token).map(Value::String);
    }

    match token {
        "true" => return Ok(Value::Bool(true)),
        "false" => return Ok(Value::Bool(false)),
        "null" => return Ok(Value::Null),
        _ => {}
    }

    if is_numeric_literal(token) {
        let number = Number::from_str(token).map_err(|_| "invalid number literal".to_string())?;
        return Ok(Value::Number(number));
    }

    Ok(Value::String(token.to_string()))
}

fn is_numeric_literal(token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    if token.starts_with('0') && token.len() > 1 && token.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    Number::from_str(token).is_ok()
}

fn split_delimited(input: &str, delimiter: Delimiter) -> Result<Vec<String>, ToonifyError> {
    let separator = delimiter.as_char();
    let mut values = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => {
                current.push(ch);
                in_quotes = !in_quotes;
            }
            '\\' if in_quotes => {
                current.push(ch);
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            _ if !in_quotes && ch == separator => {
                values.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    values.push(current.trim().to_string());
    Ok(values)
}

fn is_tabular_row_line(text: &str, delimiter: Delimiter) -> bool {
    let mut first_delim = None;
    let mut first_colon = None;
    let mut in_quotes = false;
    let mut escaped = false;
    let separator = delimiter.as_char();

    for (idx, ch) in text.char_indices() {
        if in_quotes {
            if escaped {
                escaped = false;
                continue;
            }
            match ch {
                '\\' => {
                    escaped = true;
                }
                '"' => in_quotes = false,
                _ => {}
            }
            continue;
        }

        match ch {
            '"' => in_quotes = true,
            ':' => {
                if first_colon.is_none() {
                    first_colon = Some(idx);
                }
            }
            other if other == separator => {
                if first_delim.is_none() {
                    first_delim = Some(idx);
                }
            }
            _ => {}
        }

        if first_delim.is_some() && first_colon.is_some() {
            break;
        }
    }

    match (first_delim, first_colon) {
        (None, None) => true,
        (None, Some(_)) => false,
        (Some(_), None) => true,
        (Some(delim_idx), Some(colon_idx)) => delim_idx < colon_idx,
    }
}

fn expand_paths(value: Value, strict: bool) -> Result<Value, ToonifyError> {
    match value {
        Value::Object(map) => {
            let mut replacement = Map::new();
            for (key, val) in map {
                let val = expand_paths(val, strict)?;
                if key.contains('.') && key.split('.').all(is_identifier_segment) {
                    insert_expanded(&mut replacement, &key, val, strict)?;
                } else {
                    replacement.insert(key, val);
                }
            }
            Ok(Value::Object(replacement))
        }
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(expand_paths(item, strict)?);
            }
            Ok(Value::Array(out))
        }
        other => Ok(other),
    }
}

fn insert_expanded(
    target: &mut Map<String, Value>,
    dotted: &str,
    value: Value,
    strict: bool,
) -> Result<(), ToonifyError> {
    let segments: Vec<&str> = dotted.split('.').collect();
    if segments.is_empty() {
        return Ok(());
    }
    insert_segments(target, &segments, value, strict, dotted)
}

fn insert_segments(
    current: &mut Map<String, Value>,
    segments: &[&str],
    value: Value,
    strict: bool,
    full_key: &str,
) -> Result<(), ToonifyError> {
    if segments.len() == 1 {
        match current.get_mut(segments[0]) {
            Some(existing) => {
                if strict {
                    return Err(ToonifyError::decoding(format!(
                        "expansion conflict at '{full_key}'"
                    )));
                }
                *existing = value;
            }
            None => {
                current.insert(segments[0].to_string(), value);
            }
        }
        return Ok(());
    }

    let entry = current
        .entry(segments[0].to_string())
        .or_insert_with(|| Value::Object(Map::new()));

    match entry {
        Value::Object(map) => insert_segments(map, &segments[1..], value, strict, full_key),
        other => {
            if strict {
                Err(ToonifyError::decoding(format!(
                    "expansion conflict at '{full_key}': expected object but found {other:?}"
                )))
            } else {
                *other = Value::Object(Map::new());
                if let Value::Object(map) = other {
                    insert_segments(map, &segments[1..], value, strict, full_key)
                } else {
                    unreachable!()
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn decodes_list_item_with_nested_object_first_field() {
        let doc = r#"items[1]:
  - user:
      name: Ada
      email: ada@example.com
    role: admin
"#;

        let value = decode_str(doc, DecoderOptions::default()).unwrap();
        let expected = json!({
            "items": [
                {
                    "user": {
                        "name": "Ada",
                        "email": "ada@example.com"
                    },
                    "role": "admin"
                }
            ]
        });
        assert_eq!(value, expected);
    }

    #[test]
    fn decodes_tabular_array_on_hyphen_line_and_resumes_fields() {
        let doc = r#"groups[1]:
  - members[2]{id,name}:
    1,Ada
    2,Bob
    status: active
"#;

        let value = decode_str(doc, DecoderOptions::default()).unwrap();
        let expected = json!({
            "groups": [
                {
                    "members": [
                        { "id": 1, "name": "Ada" },
                        { "id": 2, "name": "Bob" }
                    ],
                    "status": "active"
                }
            ]
        });
        assert_eq!(value, expected);
    }

    #[test]
    fn decodes_inline_array_field_inside_object() {
        let doc = r#"form:
  op[2]: readproperty,writeproperty
"#;

        let value = decode_str(doc, DecoderOptions::default()).unwrap();
        let expected = json!({
            "form": {
                "op": ["readproperty", "writeproperty"]
            }
        });
        assert_eq!(value, expected);
    }
}
