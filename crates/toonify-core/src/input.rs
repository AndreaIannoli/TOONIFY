use std::io::Read;

use csv::ReaderBuilder;
use serde_json::{Map, Value};
use xmltree::{Element, XMLNode};

use crate::error::ToonifyError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceFormat {
    Json,
    Yaml,
    Xml,
    Csv,
}

pub fn load_from_reader<R: Read>(
    mut reader: R,
    format: SourceFormat,
) -> Result<Value, ToonifyError> {
    let mut buf = String::new();
    reader.read_to_string(&mut buf)?;
    load_from_str(&buf, format)
}

pub fn load_from_str(input: &str, format: SourceFormat) -> Result<Value, ToonifyError> {
    match format {
        SourceFormat::Json => serde_json::from_str(input)
            .map_err(|err| ToonifyError::parse_err(SourceFormat::Json, err)),
        SourceFormat::Yaml => serde_yaml::from_str(input)
            .map_err(|err| ToonifyError::parse_err(SourceFormat::Yaml, err)),
        SourceFormat::Xml => parse_xml(input),
        SourceFormat::Csv => parse_csv(input),
    }
}

fn parse_csv(input: &str) -> Result<Value, ToonifyError> {
    let mut reader = ReaderBuilder::new()
        .has_headers(true)
        .trim(csv::Trim::Fields)
        .from_reader(input.as_bytes());

    let headers = reader
        .headers()
        .map_err(|err| ToonifyError::parse_err(SourceFormat::Csv, err))?
        .clone();

    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|err| ToonifyError::parse_err(SourceFormat::Csv, err))?;
        let mut row = Map::with_capacity(headers.len());
        for (idx, header) in headers.iter().enumerate() {
            let cell = record.get(idx).unwrap_or_default();
            row.insert(header.to_string(), parse_csv_cell(cell));
        }
        rows.push(Value::Object(row));
    }

    Ok(Value::Array(rows))
}

fn parse_csv_cell(cell: &str) -> Value {
    if cell.is_empty() {
        return Value::String(String::new());
    }

    if let Ok(Value::Bool(boolean)) = serde_json::from_str(cell) {
        return Value::Bool(boolean);
    }

    if let Ok(Value::Number(number)) = serde_json::from_str(cell) {
        return Value::Number(number);
    }

    if let Ok(Value::Null) = serde_json::from_str(cell) {
        return Value::Null;
    }

    Value::String(cell.to_string())
}

fn parse_xml(input: &str) -> Result<Value, ToonifyError> {
    let root = Element::parse(input.as_bytes())
        .map_err(|err| ToonifyError::parse_err(SourceFormat::Xml, err))?;

    let root_value = Value::Object({
        let mut map = Map::new();
        map.insert(root.name.clone(), element_to_value(&root));
        map
    });

    Ok(root_value)
}

fn element_to_value(element: &Element) -> Value {
    let mut object = Map::new();

    for (attr, value) in &element.attributes {
        object.insert(format!("@{}", attr), Value::String(value.clone()));
    }

    let mut child_groups: indexmap::IndexMap<String, Vec<Value>> = indexmap::IndexMap::new();
    let mut text_content = Vec::new();

    for child in &element.children {
        match child {
            XMLNode::Element(child_el) => {
                child_groups
                    .entry(child_el.name.clone())
                    .or_default()
                    .push(element_to_value(child_el));
            }
            XMLNode::Text(text) | XMLNode::CData(text) => {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    text_content.push(trimmed.to_string());
                }
            }
            _ => {}
        }
    }

    let combined_text = text_content.join(" ");
    if child_groups.is_empty() && object.is_empty() {
        if combined_text.is_empty() {
            Value::Null
        } else {
            Value::String(combined_text)
        }
    } else {
        if !combined_text.is_empty() {
            object.insert("_text".into(), Value::String(combined_text));
        }

        for (name, values) in child_groups {
            if values.len() == 1 {
                object.insert(name, values.into_iter().next().unwrap());
            } else {
                object.insert(name, Value::Array(values));
            }
        }
        Value::Object(object)
    }
}
