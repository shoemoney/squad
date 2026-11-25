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
struct CachedValidator {
    compiled: jsonschema::Validator,
    fields: std::collections::HashSet<String>,
}
type Validators = std::collections::HashMap<std::any::TypeId, std::sync::Arc<CachedValidator>>;
static VALIDATORS: std::sync::LazyLock<std::sync::RwLock<Validators>> =
    std::sync::LazyLock::new(Default::default);
pub fn decode<T: JsonSchema + DeserializeOwned + 'static>(
    value: &Value,
) -> Result<T, ValidationError> {
    let id = std::any::TypeId::of::<T>();
    let cached = VALIDATORS
        .read()
        .expect("validator cache")
        .get(&id)
        .cloned();
    let validator = match cached {
        Some(validator) => validator,
        None => {
            let mut cache = VALIDATORS.write().expect("validator cache");
            cache
                .entry(id)
                .or_insert_with(|| {
                    std::sync::Arc::new({
                        let schema = schema::<T>();
                        let mut fields = std::collections::HashSet::new();
                        collect_fields(&schema, &mut fields);
                        CachedValidator {
                            compiled: jsonschema::validator_for(&schema)
                                .expect("valid declared protocol schema"),
                            fields,
                        }
                    })
                })
                .clone()
        }
    };
    if let Err(error) = validator.compiled.validate(value) {
        // Never format the library error: its Display includes the private instance.
        let mut path = error.instance_path().to_string();
        if let jsonschema::error::ValidationErrorKind::Required { property } = error.kind()
            && let Some(property) = property.as_str()
        {
            path.push('/');
            path.push_str(property);
        }
        return Err(ValidationError {
            message: format!(
                "{}: {}",
                safe_path(&path, &validator.fields),
                error.kind().keyword()
            ),
        });
    }
    serde_path_to_error::deserialize(value.clone()).map_err(|error| {
        let mut path = String::new();
        for segment in error.path() {
            use serde_path_to_error::Segment;
            path.push('/');
            match segment {
                Segment::Map { key } => path.push_str(key),
                Segment::Seq { index } => path.push_str(&index.to_string()),
                _ => path.push('*'),
            }
        }
        ValidationError {
            message: format!(
                "{}: invalid declared value",
                safe_path(&path, &validator.fields)
            ),
        }
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
            let properties = object["properties"].as_object().expect("properties object");
            let mut required = object
                .get("required")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for (name, property) in properties {
                if property.get("x-required-nullable") == Some(&Value::Bool(true))
                    && !required.contains(&Value::String(name.clone()))
                {
                    required.push(Value::String(name.clone()));
                }
            }
            object.insert("required".into(), Value::Array(required));
            if object.get("type").is_some_and(|v| v == "object") {
                object.insert("additionalProperties".into(), Value::Bool(false));
            }
        }
        // Dispatch tagged unions by kind before validating their payload. This keeps
        // errors on the selected variant's actual field, without exposing input data.
        if let Some(branches) = object.get("oneOf").and_then(Value::as_array)
            && branches
                .iter()
                .all(|b| b.pointer("/properties/kind/const").is_some())
        {
            let branches = branches.clone();
            let kinds: Vec<_> = branches
                .iter()
                .map(|b| b["properties"]["kind"]["const"].clone())
                .collect();
            object.remove("oneOf");
            let required = object
                .entry("required")
                .or_insert_with(|| serde_json::json!([]))
                .as_array_mut()
                .expect("required array");
            if !required.contains(&Value::String("kind".into())) {
                required.push(Value::String("kind".into()));
            }
            object.insert("allOf".into(), Value::Array(branches.into_iter().map(|branch| {
                let tag = branch["properties"]["kind"].clone();
                serde_json::json!({"if":{"properties":{"kind":tag},"required":["kind"]},"then":branch})
            }).collect()));
            object
                .entry("properties")
                .or_insert_with(|| serde_json::json!({}))["kind"] =
                serde_json::json!({"enum":kinds});
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
pub fn money(value: f64) -> bool {
    value.is_finite() && value >= 0.0
}
pub fn unique<T: PartialEq>(items: &[T]) -> bool {
    items
        .iter()
        .enumerate()
        .all(|(i, item)| !items[..i].contains(item))
}

fn collect_fields(schema: &Value, fields: &mut std::collections::HashSet<String>) {
    match schema {
        Value::Object(object) => {
            if let Some(properties) = object.get("properties").and_then(Value::as_object) {
                fields.extend(properties.keys().cloned());
            }
            for child in object.values() {
                collect_fields(child, fields);
            }
        }
        Value::Array(items) => {
            for child in items {
                collect_fields(child, fields);
            }
        }
        _ => {}
    }
}
fn safe_path(path: &str, fields: &std::collections::HashSet<String>) -> String {
    let mut safe = String::new();
    for part in path.split('/').skip(1) {
        safe.push('/');
        if fields.contains(part) {
            safe.push_str(part);
        } else {
            safe.push('*');
        }
    }
    if safe.is_empty() {
        safe.push('/');
    }
    safe
}
