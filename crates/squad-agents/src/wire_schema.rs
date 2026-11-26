//! Portable subset of the response schemas, for the provider wire only.
//!
//! The room validates every reply against the full schema locally, so the schema
//! sent to a provider is guidance, never the gate. Providers reject different
//! parts of a JSON Schema with HTTP 400 (Anthropic: `not`, `patternProperties`,
//! `x-*` keys, typeless enums; Gemini: bounds that blow up its constraint
//! automaton). This reduces a schema to the intersection every default-roster
//! vendor accepts: closed objects, `$ref`/`$defs`, `anyOf`, enums and consts
//! that carry a `type`, and no bounds or string formats.
use serde_json::{Map, Value, json};

/// Keywords no default-roster vendor needs; some reject them outright.
const DROPPED: &[&str] = &[
    "$schema",
    "title",
    "not",
    "patternProperties",
    "pattern",
    "format",
    "minimum",
    "maximum",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "multipleOf",
    "minLength",
    "maxLength",
    "minItems",
    "maxItems",
    "uniqueItems",
    "minProperties",
    "maxProperties",
    "default",
    "examples",
    "if",
    "then",
    "else",
];

/// Rewrite the schema inside a `response_format` value; other formats pass through.
pub fn sanitize_response_format(format: &Value) -> Value {
    let mut format = format.clone();
    if let Some(schema) = format.pointer_mut("/json_schema/schema") {
        *schema = wire_schema(schema);
        // OpenAI strict mode cannot express map-valued objects (probe trees and
        // matrices, round scores); it answers HTTP 400. Loose mode still guides.
        if has_map(schema) {
            format["json_schema"]["strict"] = Value::Bool(false);
        }
    }
    format
}

fn has_map(schema: &Value) -> bool {
    match schema {
        Value::Object(o) => {
            o.get("additionalProperties")
                .is_some_and(|v| *v != Value::Bool(false))
                || o.iter().any(|(_, v)| has_map(v))
        }
        Value::Array(a) => a.iter().any(has_map),
        _ => false,
    }
}

pub fn wire_schema(schema: &Value) -> Value {
    match schema {
        Value::Object(object) => Value::Object(object_schema(object)),
        Value::Array(items) => Value::Array(items.iter().map(wire_schema).collect()),
        other => other.clone(),
    }
}

fn object_schema(object: &Map<String, Value>) -> Map<String, Value> {
    // A tagged union is encoded locally as `allOf` of `if/then`. On the wire it is
    // the plain union of the `then` branches.
    if let Some(Value::Array(all)) = object.get("allOf")
        && !all.is_empty()
        && all.iter().all(|b| b.get("then").is_some())
    {
        let branches = all.iter().map(|b| wire_schema(&b["then"])).collect();
        return Map::from_iter([("anyOf".to_string(), Value::Array(branches))]);
    }
    let mut out = Map::new();
    for (key, value) in object {
        if DROPPED.contains(&key.as_str()) || key.starts_with("x-") {
            continue;
        }
        // Property names and `$defs` names are data, not keywords.
        if matches!(key.as_str(), "properties" | "$defs" | "definitions")
            && let Value::Object(named) = value
        {
            out.insert(
                key.clone(),
                Value::Object(
                    named
                        .iter()
                        .map(|(name, schema)| (name.clone(), wire_schema(schema)))
                        .collect(),
                ),
            );
            continue;
        }
        out.insert(key.clone(), wire_schema(value));
    }
    if !out.contains_key("type")
        && !out.contains_key("$ref")
        && !out.contains_key("anyOf")
        && let Some(kind) = out
            .get("const")
            .map(scalar_type)
            .or_else(|| out.get("enum").and_then(enum_type))
    {
        out.insert("type".into(), json!(kind));
    }
    out
}

fn scalar_type(value: &Value) -> &'static str {
    match value {
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
        Value::Number(_) => "number",
        _ => "string",
    }
}

fn enum_type(values: &Value) -> Option<&'static str> {
    let kinds: Vec<_> = values.as_array()?.iter().map(scalar_type).collect();
    let first = *kinds.first()?;
    kinds.iter().all(|k| *k == first).then_some(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_unportable_keywords_and_types_enums() {
        let wire = wire_schema(&json!({
            "$schema": "x", "title": "T", "type": "object",
            "additionalProperties": false,
            "properties": {
                "nick": {"type": "string", "pattern": "^a", "not": {"enum": ["me"]}},
                "n": {"type": "integer", "minimum": 1, "maximum": 10, "format": "uint8"},
                "list": {"type": "array", "maxItems": 3, "items": {"type": "string"}},
                "opt": {"anyOf": [{"type": "string"}, {"type": "null"}], "x-required-nullable": true},
                "kind": {"enum": ["a", "b"]},
                "map": {"type": "object", "patternProperties": {"^a": {"type": "string"}}}
            },
            "required": ["nick"]
        }));
        assert_eq!(
            wire,
            json!({
                "type": "object", "additionalProperties": false,
                "properties": {
                    "nick": {"type": "string"},
                    "n": {"type": "integer"},
                    "list": {"type": "array", "items": {"type": "string"}},
                    "opt": {"anyOf": [{"type": "string"}, {"type": "null"}]},
                    "kind": {"enum": ["a", "b"], "type": "string"},
                    "map": {"type": "object"}
                },
                "required": ["nick"]
            })
        );
    }

    #[test]
    fn map_valued_objects_turn_strict_off() {
        let strict = json!({"type":"json_schema","json_schema":{"name":"x","strict":true,"schema":{"type":"object","additionalProperties":false,"properties":{"a":{"type":"string"}}}}});
        assert_eq!(
            sanitize_response_format(&strict)["json_schema"]["strict"],
            true
        );
        let map = json!({"type":"json_schema","json_schema":{"name":"x","strict":true,"schema":{"type":"object","additionalProperties":false,"properties":{"m":{"type":"object","additionalProperties":{"type":"string"}}}}}});
        assert_eq!(
            sanitize_response_format(&map)["json_schema"]["strict"],
            false
        );
    }

    #[test]
    fn property_named_like_a_keyword_survives() {
        let wire = wire_schema(
            &json!({"type":"object","properties":{"title":{"type":"string"},"format":{"type":"string"}}}),
        );
        assert!(
            wire["properties"]["title"].is_object() && wire["properties"]["format"].is_object()
        );
    }

    #[test]
    fn tagged_union_becomes_any_of_branches() {
        let wire = wire_schema(&json!({
            "allOf": [
                {"if": {"properties": {"kind": {"const": "a"}}}, "then": {"type": "object", "properties": {"kind": {"const": "a", "type": "string"}}, "required": ["kind"]}},
                {"if": {"properties": {"kind": {"const": "b"}}}, "then": {"type": "object", "properties": {"kind": {"const": "b"}, "x": {"type": "string"}}, "required": ["kind", "x"]}}
            ],
            "properties": {"kind": {"enum": ["a", "b"]}}, "required": ["kind"]
        }));
        let branches = wire["anyOf"].as_array().unwrap();
        assert_eq!(branches.len(), 2);
        assert_eq!(
            branches[1]["properties"]["kind"],
            json!({"const": "b", "type": "string"})
        );
        assert!(wire.get("properties").is_none());
    }
}
