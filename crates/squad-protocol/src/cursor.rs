//! Positive SQLite ordering values, represented as decimal strings on the wire.
use serde::{Deserialize, Serialize};
macro_rules! position {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(i64);
        impl $name {
            pub fn new(value:i64)->Result<Self,&'static str>{if value>0{Ok(Self(value))}else{Err("position must be positive")}}
            pub fn get(self)->i64{self.0}
        }
        impl Serialize for $name {fn serialize<S:serde::Serializer>(&self,s:S)->Result<S::Ok,S::Error>{s.serialize_str(&self.0.to_string())}}
        impl<'de> Deserialize<'de> for $name {fn deserialize<D:serde::Deserializer<'de>>(d:D)->Result<Self,D::Error>{let s=String::deserialize(d)?;if s.starts_with('0') || !s.bytes().all(|b|b.is_ascii_digit()){return Err(serde::de::Error::custom("expected positive decimal string"));}Self::new(s.parse().map_err(serde::de::Error::custom)?).map_err(serde::de::Error::custom)}}
        impl schemars::JsonSchema for $name {fn schema_name()->std::borrow::Cow<'static,str>{stringify!($name).into()}fn json_schema(_: &mut schemars::SchemaGenerator)->schemars::Schema{schemars::json_schema!({"type":"string","pattern":"^[1-9][0-9]*$"})}}
    };
}
position!(Cursor);
position!(HistoryPosition);
