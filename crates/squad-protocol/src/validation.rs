//! Schema pass rejects unknown keys and missing nullable fields before serde.
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde_json::Value;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub message: String,
}
impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.message.fmt(f)
    }
}
impl std::error::Error for ValidationError {}
pub fn ensure(condition: bool, message: impl Into<String>) -> Result<(), ValidationError> {
    if condition {
        Ok(())
    } else {
        Err(ValidationError {
            message: message.into(),
        })
    }
}
pub fn decode<T: JsonSchema + DeserializeOwned>(value: &Value) -> Result<T, ValidationError> {
    let schema = schema::<T>();
    let validator = jsonschema::validator_for(&schema).map_err(|e| ValidationError {
        message: e.to_string(),
    })?;
    if let Err(e) = validator.validate(value) {
        return Err(ValidationError {
            message: e.to_string(),
        });
    }
    serde_json::from_value(value.clone()).map_err(|e| ValidationError {
        message: e.to_string(),
    })
}
pub fn schema<T: JsonSchema>() -> Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(T)).expect("schema serialization");
    strict(&mut schema);
    schema
}
fn strict(value: &mut Value) {
    if let Value::Object(object) = value {
        // Schemars flattens tagged enums into a oneOf beside common properties.
        // Close each complete branch, rather than treating its fragment as a standalone object.
        if let Some(Value::Object(common)) = object.get("properties").cloned()
            && let Some(Value::Array(branches)) = object.get_mut("oneOf")
        {
            let mut union = common.clone();
            for branch in branches {
                if let Some(Value::Object(props)) = branch.get_mut("properties") {
                    for name in props.keys() {
                        union.entry(name.clone()).or_insert(Value::Bool(true));
                    }
                    for (name, schema) in &common {
                        props.insert(name.clone(), schema.clone());
                    }
                }
            }
            object.insert("properties".into(), Value::Object(union));
        }
        if object.get("properties").is_some_and(Value::is_object) {
            if let Some(anonymous) = object
                .get_mut("properties")
                .and_then(Value::as_object_mut)
                .and_then(|p| p.get_mut("anonymous"))
            {
                *anonymous = serde_json::json!({"const":true});
            }
            let properties = object["properties"].as_object().expect("properties object");
            let mut required: Vec<Value> = object
                .get("required")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for name in [
                "usage",
                "costUsd",
                "responseModel",
                "terminalReason",
                "tallyOverrideReason",
            ] {
                if properties.contains_key(name) && !required.contains(&Value::String(name.into()))
                {
                    required.push(Value::String(name.into()));
                }
            }
            if properties.contains_key("caseIds") {
                required.push(Value::String("caseId".into()));
            }
            if properties.contains_key("tally") && properties.contains_key("scores") {
                required.push(Value::String("scores".into()));
            }
            object.insert("required".into(), Value::Array(required));
            if object.get("type").is_some_and(|v| v == "object") {
                object.insert("additionalProperties".into(), Value::Bool(false));
            }
        }
        for (key, item) in object.iter_mut() {
            if matches!(key.as_str(), "properties" | "$defs" | "definitions") {
                if let Value::Object(map) = item {
                    for child in map.values_mut() {
                        strict(child);
                    }
                }
            } else {
                strict(item);
            }
        }
    } else if let Value::Array(items) = value {
        for item in items {
            strict(item);
        }
    }
}
pub type ValidationResult = Result<(), ValidationError>;
pub use ensure as require;
pub fn money(value: f64) -> bool {
    value.is_finite() && value >= 0.0
}
pub fn unique<T: PartialEq>(items: &[T]) -> bool {
    items
        .iter()
        .enumerate()
        .all(|(i, item)| !items[..i].contains(item))
}
