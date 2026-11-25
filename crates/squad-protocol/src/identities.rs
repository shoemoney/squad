//! Shared UUIDv7 identities. Debate identity is an alias of contest identity.
use serde::{Deserialize, Deserializer, Serialize};
use std::{fmt, str::FromStr};

macro_rules! identity {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $name(uuid::Uuid);
        impl FromStr for $name {
            type Err = &'static str;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                let id = uuid::Uuid::parse_str(value).map_err(|_| "expected UUIDv7")?;
                if id.get_version_num() != 7 || id.get_variant() != uuid::Variant::RFC4122 {
                    return Err("expected UUIDv7");
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
identity!(EventId);
identity!(MessageId);
identity!(ContestId);
pub type DebateId = ContestId;
