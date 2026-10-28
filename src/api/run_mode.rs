//! Business mode, separate from the route generation algorithm.
//! Free uses the legacy outdoor type 4 reference; type 6 is a different,
//! unverified protocol variant and is deliberately not substituted here.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunMode {
    #[default]
    Campus,
    Free,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_configs_keep_campus_and_new_mode_survives_roundtrip() {
        let mut config: crate::api::model::Config = serde_json::from_str("{}").unwrap();
        assert_eq!(config.run_mode, RunMode::Campus);
        config.run_mode = RunMode::Free;
        let saved = serde_json::to_string(&config).unwrap();
        let restored: crate::api::model::Config = serde_json::from_str(&saved).unwrap();
        assert_eq!(restored.run_mode, RunMode::Free);
        assert!(RunMode::parse("6").is_err());
    }
}

impl RunMode {
    pub fn sport_type(self) -> i64 {
        match self {
            Self::Campus => 1,
            Self::Free => 4,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Campus => "校园跑",
            Self::Free => "自由跑",
        }
    }

    pub fn is_free(self) -> bool {
        self == Self::Free
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "campus" => Ok(Self::Campus),
            "free" => Ok(Self::Free),
            _ => Err("跑步模式须为 campus 或 free".into()),
        }
    }
}
