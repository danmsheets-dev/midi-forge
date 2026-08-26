use serde::{Deserialize, Serialize};

use crate::filter::Filter;
use crate::map::DataMap;

pub const PROFILE_VERSION: u32 = 3;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProfileLink {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub filter: Filter,
    #[serde(default)]
    pub map: DataMap,
}

/// One named bench setup (thru, Lua, mute clock, throttle, MPE members).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    pub name: String,
    #[serde(default)]
    pub mute_clock: bool,
    #[serde(default)]
    pub throttle_ms: u32,
    #[serde(default = "default_mpe_members")]
    pub mpe_members: u8,
    #[serde(default)]
    pub links: Vec<ProfileLink>,
    #[serde(default)]
    pub lua: String,
    #[serde(default)]
    pub lua_enabled: bool,
}

fn default_mpe_members() -> u8 {
    15
}

impl Scene {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            mute_clock: false,
            throttle_ms: 0,
            mpe_members: 15,
            links: Vec::new(),
            lua: String::new(),
            lua_enabled: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub version: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub mute_clock: bool,
    #[serde(default)]
    pub throttle_ms: u32,
    #[serde(default = "default_mpe_members")]
    pub mpe_members: u8,
    #[serde(default)]
    pub links: Vec<ProfileLink>,
    #[serde(default)]
    pub lua: String,
    #[serde(default)]
    pub lua_enabled: bool,
    #[serde(default)]
    pub scenes: Vec<Scene>,
}

impl Profile {
    pub fn new(links: Vec<ProfileLink>) -> Self {
        Self {
            version: PROFILE_VERSION,
            name: String::new(),
            mute_clock: false,
            throttle_ms: 0,
            mpe_members: 15,
            links,
            lua: String::new(),
            lua_enabled: false,
            scenes: Vec::new(),
        }
    }

    pub fn upsert_scene(&mut self, scene: Scene) {
        if let Some(existing) = self.scenes.iter_mut().find(|s| s.name == scene.name) {
            *existing = scene;
        } else {
            self.scenes.push(scene);
        }
    }

    pub fn scene(&self, name: &str) -> Option<&Scene> {
        self.scenes.iter().find(|s| s.name == name)
    }

    /// Current thru/lua as a scene (uses `name`, or "Default").
    pub fn current_scene(&self) -> Scene {
        Scene {
            name: if self.name.is_empty() {
                "Default".into()
            } else {
                self.name.clone()
            },
            mute_clock: self.mute_clock,
            throttle_ms: self.throttle_ms,
            mpe_members: self.mpe_members,
            links: self.links.clone(),
            lua: self.lua.clone(),
            lua_enabled: self.lua_enabled,
        }
    }

    pub fn apply_scene(&mut self, scene: &Scene) {
        self.name = scene.name.clone();
        self.mute_clock = scene.mute_clock;
        self.throttle_ms = scene.throttle_ms;
        self.mpe_members = scene.mpe_members;
        self.links = scene.links.clone();
        self.lua = scene.lua.clone();
        self.lua_enabled = scene.lua_enabled;
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
        assert_eq!(profile.mpe_members, 15);
    }

    #[test]
    fn scene_upsert_and_recall() {
        let mut p = Profile::new(vec![]);
        p.mute_clock = true;
        p.name = "Keys A".into();
        let scene = p.current_scene();
        p.upsert_scene(scene.clone());
        p.mute_clock = false;
        p.name.clear();
        let recalled = p.scene("Keys A").unwrap().clone();
        p.apply_scene(&recalled);
        assert!(p.mute_clock);
        assert_eq!(p.name, "Keys A");
        let json = p.to_json().unwrap();
        assert!(json.contains("Keys A"));
        assert_eq!(Profile::from_json(&json).unwrap().scenes.len(), 1);
    }
}
