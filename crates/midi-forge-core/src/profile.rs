use serde::{Deserialize, Serialize};

use crate::filter::Filter;
use crate::map::DataMap;

pub const PROFILE_VERSION: u32 = 2;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProfileLink {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub filter: Filter,
    #[serde(default)]
    pub map: DataMap,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub version: u32,
    #[serde(default)]
    pub links: Vec<ProfileLink>,
    #[serde(default)]
    pub lua: String,
    #[serde(default)]
    pub lua_enabled: bool,
}

impl Profile {
    pub fn new(links: Vec<ProfileLink>) -> Self {
        Self {
            version: PROFILE_VERSION,
            links,
            lua: String::new(),
            lua_enabled: false,
        }
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::DataMap;

    #[test]
    fn json_roundtrip_preserves_transpose_map() {
        let profile = Profile::new(vec![ProfileLink {
            from: "winmm:in:0".into(),
            to: "winmm:out:0".into(),
            filter: Filter {
                clock: false,
                ..Filter::default()
            },
            map: DataMap::transpose(12),
        }]);
        let json = profile.to_json().unwrap();
        let loaded = Profile::from_json(&json).unwrap();
        assert_eq!(loaded, profile);
        assert!(json.contains("winmm:in:0"));
        assert!(json.contains("offset"));
    }

    #[test]
    fn v1_json_loads_with_empty_lua() {
        let json = r#"{"version":1,"links":[]}"#;
        let profile = Profile::from_json(json).unwrap();
        assert!(profile.lua.is_empty());
        assert!(!profile.lua_enabled);
    }
}
