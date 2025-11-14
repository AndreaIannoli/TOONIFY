use std::borrow::Cow;
use std::str::FromStr;

use bigdecimal::{BigDecimal, Zero};
use serde_json::{Map, Number, Value};

use crate::error::ToonifyError;
use crate::options::{Delimiter, EncoderOptions, KeyFoldingMode};
use crate::quoting::{encode_key, encode_string, is_identifier_segment};

pub fn encode_value(value: &Value, options: &EncoderOptions) -> Result<String, ToonifyError> {
    let mut encoder = Encoder::new(options);
    encoder.encode_root(value)?;
    Ok(encoder.finish())
}

struct Encoder<'a> {
    options: &'a EncoderOptions,
    lines: Vec<String>,
}

impl<'a> Encoder<'a> {
    fn new(options: &'a EncoderOptions) -> Self {
        Self {
            options,
            lines: Vec::new(),
        }
    }

    fn finish(self) -> String {
        self.lines.join("\n")
    }

    fn encode_root(&mut self, value: &Value) -> Result<(), ToonifyError> {
        match value {
            Value::Object(map) => {
                if map.is_empty() {
                    Ok(())
                } else {
                    self.encode_object_fields(map, 0)
                }
            }
            Value::Array(items) => {
                self.encode_array(None, items, ArrayContext::Normal { depth: 0 })
            }
            primitive => {
                let rendered =
                    self.stringify_primitive(primitive, Some(self.options.document_delimiter))?;
                self.lines.push(rendered);
                Ok(())
            }
        }
    }

    fn encode_object_fields(
        &mut self,
        map: &Map<String, Value>,
        depth: usize,
    ) -> Result<(), ToonifyError> {
        for (key, value) in map {
            let FoldResult { key, value } = self.fold_key(key, value, map);
            self.encode_named_value(&key, value, depth)?;
        }
        Ok(())
    }

    fn encode_named_value(
        &mut self,
        key: &str,
        value: &Value,
        depth: usize,
    ) -> Result<(), ToonifyError> {
        match value {
            Value::Object(map) => {
                if map.is_empty() {
                    self.push_line(depth, format!("{}:", encode_key(key)));
                } else {
                    self.push_line(depth, format!("{}:", encode_key(key)));
                    self.encode_object_fields(map, depth + 1)?;
                }
            }
            Value::Array(items) => {
                self.encode_array(Some(key), items, ArrayContext::Normal { depth })?
            }
            primitive => {
                let rendered =
                    self.stringify_primitive(primitive, Some(self.options.document_delimiter))?;
                self.push_line(depth, format!("{}: {}", encode_key(key), rendered));
            }
        }
        Ok(())
    }

    fn encode_array(
        &mut self,
        key: Option<&str>,
        items: &[Value],
        context: ArrayContext,
    ) -> Result<(), ToonifyError> {
        let delimiter = self.options.document_delimiter;
        if items.iter().all(is_primitive) {
            self.emit_inline_array(key, items, delimiter, context)?;
            return Ok(());
        }

        if let Some(fields) = detect_tabular(items) {
            self.emit_tabular_array(key, items, &fields, delimiter, context)?;
            return Ok(());
        }

        if is_array_of_primitive_arrays(items) {
            self.emit_array_of_arrays(key, items, delimiter, context)?;
            return Ok(());
        }

        self.emit_general_list(key, items, delimiter, context)
    }

    fn emit_inline_array(
        &mut self,
        key: Option<&str>,
        items: &[Value],
        delimiter: Delimiter,
        context: ArrayContext,
    ) -> Result<(), ToonifyError> {
        let header = self.format_header(key, items.len(), delimiter, None);
        let indent = self.indent(context.header_depth());
        let prefix = context.header_prefix();

        if items.is_empty() {
            self.lines.push(format!("{}{}{}", indent, prefix, header));
        } else {
            let sep = delimiter.separator().to_string();
            let values = items
                .iter()
                .map(|value| self.stringify_primitive(value, Some(delimiter)))
                .collect::<Result<Vec<_>, _>>()?;
            let joined = values.join(&sep);
            self.lines
                .push(format!("{}{}{} {}", indent, prefix, header, joined));
        }
        Ok(())
    }

    fn emit_tabular_array(
        &mut self,
        key: Option<&str>,
        items: &[Value],
        fields: &[String],
        delimiter: Delimiter,
        context: ArrayContext,
    ) -> Result<(), ToonifyError> {
        let header = self.format_header(key, items.len(), delimiter, Some(fields));
        let indent = self.indent(context.header_depth());
        let prefix = context.header_prefix();
        self.lines.push(format!("{}{}{}", indent, prefix, header));

        let row_indent_depth = context.row_depth();
        let row_indent = self.indent(row_indent_depth);
        let sep = delimiter.separator().to_string();

        for item in items {
            let obj = item.as_object().ok_or_else(|| {
                ToonifyError::encoding("tabular detection failed due to non-object row")
            })?;
            let mut cells = Vec::with_capacity(fields.len());
            for field in fields {
                let cell = obj.get(field).expect("field must exist");
                let rendered = self.stringify_primitive(cell, Some(delimiter))?;
                cells.push(rendered);
            }
            self.lines
                .push(format!("{}{}", row_indent, cells.join(&sep)));
        }

        Ok(())
    }

    fn emit_array_of_arrays(
        &mut self,
        key: Option<&str>,
        items: &[Value],
        delimiter: Delimiter,
        context: ArrayContext,
    ) -> Result<(), ToonifyError> {
        let header = self.format_header(key, items.len(), delimiter, None);
        let indent = self.indent(context.header_depth());
        let prefix = context.header_prefix();
        self.lines.push(format!("{}{}{}", indent, prefix, header));

        for inner in items {
            let inner_items = inner
                .as_array()
                .ok_or_else(|| ToonifyError::encoding("expected inner array"))?;
            let inner_header = self.format_header(None, inner_items.len(), delimiter, None);
            let row_indent = self.indent(context.row_depth());
            if inner_items.is_empty() {
                self.lines.push(format!("{}- {}", row_indent, inner_header));
            } else {
                let sep = delimiter.separator().to_string();
                let values = inner_items
                    .iter()
                    .map(|value| self.stringify_primitive(value, Some(delimiter)))
                    .collect::<Result<Vec<_>, _>>()?;
                let joined = values.join(&sep);
                self.lines
                    .push(format!("{}- {} {}", row_indent, inner_header, joined));
            }
        }

        Ok(())
    }

    fn emit_general_list(
        &mut self,
        key: Option<&str>,
        items: &[Value],
        delimiter: Delimiter,
        context: ArrayContext,
    ) -> Result<(), ToonifyError> {
        let header = self.format_header(key, items.len(), delimiter, None);
        let indent = self.indent(context.header_depth());
        let prefix = context.header_prefix();
        self.lines.push(format!("{}{}{}", indent, prefix, header));
        let row_indent_depth = context.row_depth();

        for item in items {
            match item {
                Value::Object(map) => self.encode_object_list_item(map, row_indent_depth)?,
                Value::Array(inner) => {
                    self.encode_array(
                        None,
                        inner,
                        ArrayContext::ListFirstField {
                            depth: row_indent_depth.saturating_sub(1),
                        },
                    )?;
                }
                primitive => {
                    let rendered =
                        self.stringify_primitive(primitive, Some(self.options.document_delimiter))?;
                    let indent = self.indent(row_indent_depth);
                    self.lines.push(format!("{}- {}", indent, rendered));
                }
            }
        }

        Ok(())
    }

    fn encode_object_list_item(
        &mut self,
        map: &Map<String, Value>,
        depth: usize,
    ) -> Result<(), ToonifyError> {
        if map.is_empty() {
            let indent = self.indent(depth);
            self.lines.push(format!("{}-", indent));
            return Ok(());
        }

        let mut iter = map.iter();
        if let Some((first_key, first_value)) = iter.next() {
            let FoldResult { key, value } = self.fold_key(first_key, first_value, map);
            match value {
                Value::Object(obj) => {
                    let indent = self.indent(depth);
                    self.lines
                        .push(format!("{}- {}:", indent, encode_key(&key)));
                    if !obj.is_empty() {
                        self.encode_object_fields(obj, depth + 2)?;
                    }
                }
                Value::Array(items) => {
                    self.encode_array(
                        Some(&key),
                        items,
                        ArrayContext::ListFirstField {
                            depth: depth.saturating_sub(1),
                        },
                    )?;
                }
                primitive => {
                    let indent = self.indent(depth);
                    let rendered =
                        self.stringify_primitive(primitive, Some(self.options.document_delimiter))?;
                    self.lines
                        .push(format!("{}- {}: {}", indent, encode_key(&key), rendered));
                }
            }

            for (key, value) in iter {
                let FoldResult { key, value } = self.fold_key(key, value, map);
                self.encode_named_value(&key, value, depth + 1)?;
            }
        }
        Ok(())
    }

    fn stringify_primitive(
        &self,
        value: &Value,
        delimiter: Option<Delimiter>,
    ) -> Result<String, ToonifyError> {
        let delimiter = delimiter.unwrap_or(self.options.document_delimiter);
        match value {
            Value::Null => Ok("null".into()),
            Value::Bool(boolean) => Ok(boolean.to_string()),
            Value::Number(number) => self.canonicalize_number(number),
            Value::String(text) => Ok(encode_string(text, Some(delimiter))),
            other => Err(ToonifyError::encoding(format!(
                "expected primitive value, found {other:?}"
            ))),
        }
    }

    fn canonicalize_number(&self, number: &Number) -> Result<String, ToonifyError> {
        if let Some(value) = number.as_i64() {
            return Ok(value.to_string());
        }
        if let Some(value) = number.as_u64() {
            return Ok(value.to_string());
        }

        let raw = number.to_string();
        if raw == "-0" {
            return Ok("0".into());
        }

        let decimal =
            BigDecimal::from_str(&raw).map_err(|err| ToonifyError::NumberNormalization {
                value: raw.clone(),
                source: Box::new(err),
            })?;

        let normalized = decimal.normalized();
        if normalized.is_zero() {
            Ok("0".into())
        } else {
            Ok(normalized.to_string())
        }
    }

    fn format_header(
        &self,
        key: Option<&str>,
        len: usize,
        delimiter: Delimiter,
        fields: Option<&[String]>,
    ) -> String {
        let bracket = format!("[{}{}]", len, delimiter.bracket_suffix());
        let body = if let Some(fields) = fields {
            let sep = delimiter.as_char().to_string();
            let field_list = fields
                .iter()
                .map(|field| encode_key(field))
                .collect::<Vec<_>>()
                .join(&sep);
            format!("{bracket}{{{field_list}}}:")
        } else {
            format!("{bracket}:")
        };

        match key {
            Some(key) => format!("{}{}", encode_key(key), body),
            None => body,
        }
    }

    fn fold_key<'m>(
        &self,
        key: &'m str,
        value: &'m Value,
        siblings: &'m Map<String, Value>,
    ) -> FoldResult<'m> {
        let KeyFoldingMode::Safe { flatten_depth } = self.options.key_folding else {
            return FoldResult::borrowed(key, value);
        };

        if !is_identifier_segment(key) {
            return FoldResult::borrowed(key, value);
        }

        let max_segments = flatten_depth.unwrap_or(usize::MAX).max(1);
        let mut segments = vec![key.to_string()];
        let mut current = value;

        while segments.len() < max_segments {
            match current {
                Value::Object(map) if map.len() == 1 => {
                    let (next_key, next_value) = map.iter().next().unwrap();
                    if !is_identifier_segment(next_key) {
                        break;
                    }
                    segments.push(next_key.to_string());
                    current = next_value;
                }
                _ => break,
            }
        }

        if segments.len() == 1 {
            return FoldResult::borrowed(key, value);
        }

        let candidate = segments.join(".");
        if siblings.contains_key(&candidate) && candidate != key {
            return FoldResult::borrowed(key, value);
        }

        FoldResult::owned(candidate, current)
    }

    fn push_line(&mut self, depth: usize, content: String) {
        let indent = self.indent(depth);
        self.lines.push(format!("{indent}{content}"));
    }

    fn indent(&self, depth: usize) -> String {
        " ".repeat(depth * self.options.indent)
    }
}

struct FoldResult<'a> {
    key: Cow<'a, str>,
    value: &'a Value,
}

impl<'a> FoldResult<'a> {
    fn borrowed(key: &'a str, value: &'a Value) -> Self {
        Self {
            key: Cow::Borrowed(key),
            value,
        }
    }

    fn owned(key: String, value: &'a Value) -> Self {
        Self {
            key: Cow::Owned(key),
            value,
        }
    }
}

#[derive(Clone, Copy)]
enum ArrayContext {
    Normal { depth: usize },
    ListFirstField { depth: usize },
}

impl ArrayContext {
    fn header_depth(self) -> usize {
        match self {
            ArrayContext::Normal { depth } => depth,
            ArrayContext::ListFirstField { depth } => depth + 1,
        }
    }

    fn row_depth(self) -> usize {
        self.header_depth() + 1
    }

    fn header_prefix(self) -> &'static str {
        match self {
            ArrayContext::Normal { .. } => "",
            ArrayContext::ListFirstField { .. } => "- ",
        }
    }
}

fn is_primitive(value: &Value) -> bool {
    matches!(
        value,
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_)
    )
}

fn detect_tabular(items: &[Value]) -> Option<Vec<String>> {
    if items.is_empty() {
        return None;
    }

    let first = items.get(0)?.as_object()?;
    if first.is_empty() {
        return None;
    }

    let mut fields = Vec::new();
    for (key, value) in first {
        if !is_primitive(value) {
            return None;
        }
        fields.push(key.clone());
    }

    for item in items.iter().skip(1) {
        let obj = item.as_object()?;
        if obj.len() != fields.len() {
            return None;
        }
        for field in &fields {
            let value = obj.get(field)?;
            if !is_primitive(value) {
                return None;
            }
        }
    }

    Some(fields)
}

fn is_array_of_primitive_arrays(items: &[Value]) -> bool {
    !items.is_empty()
        && items.iter().all(|value| {
            value
                .as_array()
                .map(|inner| inner.iter().all(is_primitive))
                .unwrap_or(false)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::{Delimiter, EncoderOptions, KeyFoldingMode};
    use serde_json::json;

    #[test]
    fn encodes_object_and_tabular_array() {
        let value = json!({
            "users": [
                { "id": 1, "name": "Ada", "active": true },
                { "id": 2, "name": "Linus", "active": false }
            ],
            "count": 2
        });

        let output = encode_value(&value, &EncoderOptions::default()).unwrap();
        assert_eq!(
            output,
            "users[2]{id,name,active}:\n  1,Ada,true\n  2,Linus,false\ncount: 2"
        );
    }

    #[test]
    fn folds_keys_when_enabled() {
        let options = EncoderOptions {
            indent: 2,
            document_delimiter: Delimiter::Comma,
            key_folding: KeyFoldingMode::Safe {
                flatten_depth: None,
            },
        };

        let value = json!({
            "data": {
                "meta": {
                    "payload": {
                        "id": 1
                    }
                }
            }
        });

        let output = encode_value(&value, &options).unwrap();
        assert_eq!(output, "data.meta.payload.id: 1");
    }
}
