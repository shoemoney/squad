//! Version-checked, distinct identities and validated string primitives.
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Deserializer, Serialize};
use std::{fmt, str::FromStr};
macro_rules! identity {
    ($name:ident, $version:literal, $new:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, JsonSchema,
        )]
        #[serde(transparent)]
        pub struct $name(uuid::Uuid);
        impl $name {
            pub fn new() -> Self {
                Self(uuid::Uuid::$new())
            }
            pub fn as_uuid(&self) -> &uuid::Uuid {
                &self.0
            }
        }
        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
        impl FromStr for $name {
            type Err = &'static str;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                let id = uuid::Uuid::parse_str(value).map_err(|_| "invalid UUID")?;
                if id.get_version_num() != $version || id.get_variant() != uuid::Variant::RFC4122 {
                    return Err(concat!("expected UUIDv", stringify!($version)));
                }
                Ok(Self(id))
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                String::deserialize(d)?
                    .parse()
                    .map_err(serde::de::Error::custom)
            }
        }
    };
}
identity!(EventId, 7, now_v7);
identity!(MessageId, 7, now_v7);
identity!(WorkId, 7, now_v7);
identity!(ContestId, 7, now_v7);
identity!(CaseId, 7, now_v7);
identity!(PlanId, 7, now_v7);
identity!(StepId, 7, now_v7);
identity!(CellId, 4, new_v4);
pub type DebateId = ContestId;

impl JsonSchema for crate::Nick {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "MemberNick".into()
    }
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        schemars::json_schema!({"type":"string", "pattern":"^[a-z][a-z0-9_]{0,31}$", "not":{"enum":["me","all","judge","system","squad"]}})
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(transparent)]
pub struct Text(String);
impl Text {
    pub fn new(value: impl Into<String>) -> Result<Self, &'static str> {
        let value = value.into();
        if value.is_empty() {
            Err("text must be nonempty")
        } else {
            Ok(Self(value))
        }
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl fmt::Display for Text {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl FromStr for Text {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}
impl<'de> Deserialize<'de> for Text {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct ContextId(pub String);
macro_rules! actor {
    ($name:ident, $($reserved:ident => $wire:literal),*) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub enum $name { Member(crate::MemberNick), $($reserved),* }
        impl fmt::Display for $name { fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result {match self {Self::Member(n)=>n.fmt(f),$(Self::$reserved=>f.write_str($wire)),*}} }
        impl Serialize for $name { fn serialize<S:serde::Serializer>(&self,s:S)->Result<S::Ok,S::Error>{s.serialize_str(&self.to_string())} }
        impl<'de> Deserialize<'de> for $name { fn deserialize<D:Deserializer<'de>>(d:D)->Result<Self,D::Error>{let s=String::deserialize(d)?;match s.as_str() {$($wire=>Ok(Self::$reserved),)*_=>serde_json::from_value(serde_json::Value::String(s)).map(Self::Member).map_err(serde::de::Error::custom)}} }
        impl JsonSchema for $name {fn schema_name()->std::borrow::Cow<'static,str>{stringify!($name).into()} fn json_schema(g:&mut SchemaGenerator)->Schema{let nick=g.subschema_for::<crate::MemberNick>();schemars::json_schema!({"anyOf":[nick,{"enum":[$($wire),*]}]})}}
    };
}
actor!(Actor, Me => "me", System => "system", Judge => "judge");
actor!(Recipient, Me => "me", Judge => "judge");
actor!(Target, Judge => "judge");

impl JsonSchema for crate::VariableName {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "VariableName".into()
    }
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        schemars::json_schema!({"enum":["ROUNDS","GRILL_ROUNDS","CENSUS_PASSES"]})
    }
}
