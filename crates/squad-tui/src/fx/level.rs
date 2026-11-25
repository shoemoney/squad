//! Explicit animation policy, independent of colour support.
use std::{fmt, str::FromStr};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FxLevel {
    Off,
    Lite,
    #[default]
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidFxLevel;
impl fmt::Display for InvalidFxLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SQUAD_FX must be full, lite or off")
    }
}
impl std::error::Error for InvalidFxLevel {}
impl FromStr for FxLevel {
    type Err = InvalidFxLevel;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "full" => Ok(Self::Full),
            "lite" => Ok(Self::Lite),
            "off" => Ok(Self::Off),
            _ => Err(InvalidFxLevel),
        }
    }
}
impl FxLevel {
    pub fn resolve(
        explicit: Option<&str>,
        ssh: bool,
        synchronized: bool,
    ) -> Result<Self, InvalidFxLevel> {
        explicit.map(str::parse).unwrap_or_else(|| {
            Ok(if ssh || !synchronized {
                Self::Lite
            } else {
                Self::Full
            })
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn values_and_invalid_input() {
        for (value, expected) in [
            ("full", FxLevel::Full),
            ("lite", FxLevel::Lite),
            ("off", FxLevel::Off),
        ] {
            assert_eq!(value.parse(), Ok(expected));
        }
        for value in ["", "Full", "auto", "garbage", " full"] {
            assert!(value.parse::<FxLevel>().is_err());
        }
        assert_eq!(FxLevel::default(), FxLevel::Full);
    }
}
