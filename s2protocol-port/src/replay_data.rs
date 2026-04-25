use crate::error::DecodeError;
use crate::replay_format::ReplayFormat;
use crate::value::Value;
use std::collections::{BTreeMap, HashMap};

const DEFAULT_GAME_SPEED_CODE: i64 = 4;

#[allow(non_snake_case)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReplayVersion {
    pub m_baseBuild: u32,
    pub m_version: Option<u32>,
}

#[allow(non_snake_case)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReplayHeader {
    pub m_version: ReplayVersion,
    pub m_elapsedGameLoops: Option<u64>,
    pub m_useScaledTime: bool,
}

#[allow(non_snake_case)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReplayToon {
    pub m_region: i64,
}

#[allow(non_snake_case)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReplayDetailsPlayer {
    pub m_name: String,
    pub m_race: String,
    pub m_observe: i64,
    pub m_result: String,
    pub m_toon: Option<ReplayToon>,
}

#[allow(non_snake_case)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayDetails {
    pub m_playerList: Vec<ReplayDetailsPlayer>,
    pub m_isBlizzardMap: bool,
    pub m_disableRecoverGame: Option<bool>,
    pub m_cacheHandles: Vec<String>,
    pub m_gameSpeed: i64,
}

#[allow(non_snake_case)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReplayUserInitialData {
    pub m_name: String,
}

#[allow(non_snake_case)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReplayLobbySlot {
    pub m_brutalPlusDifficulty: i64,
    pub m_retryMutationIndexes: Vec<i64>,
    pub m_commander: String,
    pub m_commanderLevel: i64,
    pub m_commanderMasteryLevel: i64,
    pub m_selectedCommanderPrestige: i64,
    pub m_toonHandle: String,
    pub m_commanderMasteryTalents: Vec<u32>,
    pub m_race: String,
    pub m_difficulty: i64,
}

#[allow(non_snake_case)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReplayLobbyState {
    pub m_slots: Vec<ReplayLobbySlot>,
}

#[allow(non_snake_case)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayGameDescription {
    pub m_hasExtensionMod: bool,
    pub m_cacheHandles: Vec<String>,
    pub m_gameSpeed: i64,
}

#[allow(non_snake_case)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReplaySyncLobbyState {
    pub m_userInitialData: Vec<ReplayUserInitialData>,
    pub m_gameDescription: ReplayGameDescription,
    pub m_lobbyState: ReplayLobbyState,
}

#[allow(non_snake_case)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReplayInitData {
    pub m_syncLobbyState: ReplaySyncLobbyState,
}

#[allow(non_snake_case)]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReplayMetadataPlayer {
    pub APM: f64,
    pub Result: String,
}

#[allow(non_snake_case)]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReplayMetadata {
    pub Title: String,
    pub Duration: f64,
    pub Players: Vec<ReplayMetadataPlayer>,
}

#[allow(non_snake_case)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReplayAttributeValue {
    pub namespace: u32,
    pub attrid: u32,
    pub scope: u8,
    pub value: Vec<u8>,
}

#[allow(non_snake_case)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReplayAttributes {
    pub source: u8,
    pub mapNamespace: u32,
    pub count: u32,
    pub scopes: BTreeMap<String, BTreeMap<String, Vec<ReplayAttributeValue>>>,
}

#[allow(non_snake_case)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReplayAttributeScope {
    pub scope: String,
    pub values: BTreeMap<String, Vec<u8>>,
}

impl ReplayHeader {
    pub fn base_build(&self) -> u32 {
        self.m_version.m_baseBuild
    }

    pub(crate) fn from_value(value: Value) -> Result<Self, DecodeError> {
        let map = ReplayValueReader::as_object(&value)?;
        let version = map
            .get("m_version")
            .ok_or_else(|| DecodeError::Corrupted("missing m_version".into()))?;
        let version_map = ReplayValueReader::as_object(version)?;
        let base_build = ReplayValueReader::get_u32(version_map, "m_baseBuild")
            .or_else(|| ReplayValueReader::get_u32(version_map, "m_version"))
            .ok_or_else(|| DecodeError::Corrupted("missing m_version.m_baseBuild".into()))?;
        let m_version = ReplayValueReader::get_u32(version_map, "m_version");
        Ok(Self {
            m_version: ReplayVersion {
                m_baseBuild: base_build,
                m_version,
            },
            m_elapsedGameLoops: ReplayValueReader::get_u64(map, "m_elapsedGameLoops"),
            m_useScaledTime: ReplayValueReader::get_bool(map, "m_useScaledTime").unwrap_or(false),
        })
    }
}

impl Default for ReplayDetails {
    fn default() -> Self {
        Self {
            m_playerList: Vec::new(),
            m_isBlizzardMap: false,
            m_disableRecoverGame: None,
            m_cacheHandles: Vec::new(),
            m_gameSpeed: DEFAULT_GAME_SPEED_CODE,
        }
    }
}

impl ReplayDetails {
    pub(crate) fn from_value(value: Value) -> Result<Self, DecodeError> {
        let map = ReplayValueReader::as_object(&value)?;
        let game_speed = ReplayValueReader::get_i64(map, "m_gameSpeed");
        Ok(Self {
            m_playerList: map
                .get("m_playerList")
                .and_then(ReplayValueReader::as_array_opt)
                .unwrap_or_default()
                .iter()
                .map(ReplayDetailsPlayer::from_value)
                .collect::<Result<Vec<_>, _>>()?,
            m_isBlizzardMap: ReplayValueReader::get_bool(map, "m_isBlizzardMap").unwrap_or(false),
            m_disableRecoverGame: ReplayValueReader::get_bool(map, "m_disableRecoverGame"),
            m_cacheHandles: map
                .get("m_cacheHandles")
                .and_then(ReplayValueReader::as_array_opt)
                .map(ReplayValueReader::parse_cache_handles)
                .unwrap_or_default(),
            m_gameSpeed: game_speed.unwrap_or(DEFAULT_GAME_SPEED_CODE),
        })
    }
}

impl ReplayDetailsPlayer {
    fn from_value(value: &Value) -> Result<Self, DecodeError> {
        let map = ReplayValueReader::as_object(value)?;
        Ok(Self {
            m_name: ReplayValueReader::get_string(map, "m_name").unwrap_or_default(),
            m_race: ReplayValueReader::get_string(map, "m_race").unwrap_or_default(),
            m_observe: ReplayValueReader::get_i64(map, "m_observe").unwrap_or_default(),
            m_result: ReplayValueReader::get_string(map, "m_result").unwrap_or_default(),
            m_toon: map.get("m_toon").map(ReplayToon::from_value).transpose()?,
        })
    }
}

impl ReplayToon {
    fn from_value(value: &Value) -> Result<Self, DecodeError> {
        let map = ReplayValueReader::as_object(value)?;
        Ok(Self {
            m_region: ReplayValueReader::get_i64(map, "m_region").unwrap_or_default(),
        })
    }
}

impl ReplayInitData {
    pub(crate) fn from_value(value: Value) -> Result<Self, DecodeError> {
        let map = ReplayValueReader::as_object(&value)?;
        let sync = map
            .get("m_syncLobbyState")
            .ok_or_else(|| DecodeError::Corrupted("missing m_syncLobbyState".into()))?;
        Ok(Self {
            m_syncLobbyState: ReplaySyncLobbyState::from_value(sync)?,
        })
    }
}

impl ReplaySyncLobbyState {
    fn from_value(value: &Value) -> Result<Self, DecodeError> {
        let map = ReplayValueReader::as_object(value)?;
        Ok(Self {
            m_userInitialData: map
                .get("m_userInitialData")
                .and_then(ReplayValueReader::as_array_opt)
                .unwrap_or_default()
                .iter()
                .map(ReplayUserInitialData::from_value)
                .collect::<Result<Vec<_>, _>>()?,
            m_gameDescription: map
                .get("m_gameDescription")
                .map(ReplayGameDescription::from_value)
                .transpose()?
                .unwrap_or_default(),
            m_lobbyState: map
                .get("m_lobbyState")
                .map(ReplayLobbyState::from_value)
                .transpose()?
                .unwrap_or_default(),
        })
    }
}

impl ReplayUserInitialData {
    fn from_value(value: &Value) -> Result<Self, DecodeError> {
        let map = ReplayValueReader::as_object(value)?;
        Ok(Self {
            m_name: ReplayValueReader::get_string(map, "m_name").unwrap_or_default(),
        })
    }
}

impl Default for ReplayGameDescription {
    fn default() -> Self {
        Self {
            m_hasExtensionMod: false,
            m_cacheHandles: Vec::new(),
            m_gameSpeed: DEFAULT_GAME_SPEED_CODE,
        }
    }
}

impl ReplayGameDescription {
    fn from_value(value: &Value) -> Result<Self, DecodeError> {
        let map = ReplayValueReader::as_object(value)?;
        let game_speed = ReplayValueReader::get_i64(map, "m_gameSpeed");
        Ok(Self {
            m_hasExtensionMod: ReplayValueReader::get_bool(map, "m_hasExtensionMod")
                .unwrap_or(false),
            m_cacheHandles: map
                .get("m_cacheHandles")
                .and_then(ReplayValueReader::as_array_opt)
                .map(ReplayValueReader::parse_cache_handles)
                .unwrap_or_default(),
            m_gameSpeed: game_speed.unwrap_or(DEFAULT_GAME_SPEED_CODE),
        })
    }
}

impl ReplayLobbyState {
    fn from_value(value: &Value) -> Result<Self, DecodeError> {
        let map = ReplayValueReader::as_object(value)?;
        Ok(Self {
            m_slots: map
                .get("m_slots")
                .and_then(ReplayValueReader::as_array_opt)
                .unwrap_or_default()
                .iter()
                .map(ReplayLobbySlot::from_value)
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

impl ReplayLobbySlot {
    fn from_value(value: &Value) -> Result<Self, DecodeError> {
        let map = ReplayValueReader::as_object(value)?;
        Ok(Self {
            m_brutalPlusDifficulty: ReplayValueReader::get_i64(map, "m_brutalPlusDifficulty")
                .unwrap_or_default(),
            m_retryMutationIndexes: map
                .get("m_retryMutationIndexes")
                .and_then(ReplayValueReader::as_array_opt)
                .unwrap_or_default()
                .iter()
                .filter_map(ReplayValueReader::value_as_i64)
                .collect(),
            m_commander: ReplayValueReader::get_string(map, "m_commander").unwrap_or_default(),
            m_commanderLevel: ReplayValueReader::get_i64(map, "m_commanderLevel")
                .unwrap_or_default(),
            m_commanderMasteryLevel: ReplayValueReader::get_i64(map, "m_commanderMasteryLevel")
                .unwrap_or_default(),
            m_selectedCommanderPrestige: ReplayValueReader::get_i64(
                map,
                "m_selectedCommanderPrestige",
            )
            .unwrap_or_default(),
            m_toonHandle: ReplayValueReader::get_string(map, "m_toonHandle").unwrap_or_default(),
            m_commanderMasteryTalents: map
                .get("m_commanderMasteryTalents")
                .and_then(ReplayValueReader::as_array_opt)
                .unwrap_or_default()
                .iter()
                .filter_map(ReplayValueReader::value_as_i64)
                .filter_map(|value| u32::try_from(value).ok())
                .collect(),
            m_race: ReplayValueReader::get_string(map, "m_race").unwrap_or_default(),
            m_difficulty: ReplayValueReader::get_i64(map, "m_difficulty").unwrap_or_default(),
        })
    }
}

impl ReplayMetadata {
    pub(crate) fn from_json_value(value: serde_json::Value) -> Result<Self, DecodeError> {
        let object = value
            .as_object()
            .ok_or_else(|| DecodeError::Corrupted("metadata must be object".into()))?;
        Ok(Self {
            Title: object
                .get("Title")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string(),
            Duration: object
                .get("Duration")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or_default(),
            Players: object
                .get("Players")
                .and_then(serde_json::Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or(&[])
                .iter()
                .map(ReplayMetadataPlayer::from_json_value)
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

impl ReplayMetadataPlayer {
    fn from_json_value(value: &serde_json::Value) -> Result<Self, DecodeError> {
        let object = value
            .as_object()
            .ok_or_else(|| DecodeError::Corrupted("metadata player must be object".into()))?;
        Ok(Self {
            APM: object
                .get("APM")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or_default(),
            Result: object
                .get("Result")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string(),
        })
    }
}

impl ReplayAttributes {
    pub fn scope_attributes(&self) -> Vec<ReplayAttributeScope> {
        let attribute_name_map = ReplayValueReader::parse_attribute_name_map();
        let mut out = Vec::new();
        for (scope, attrs) in &self.scopes {
            let mut values = BTreeMap::new();
            for (attribute_id, raw_values) in attrs {
                let value = raw_values
                    .first()
                    .map(|entry| entry.value.clone())
                    .unwrap_or_default();
                let symbolic =
                    ReplayValueReader::attribute_id_to_name(&attribute_name_map, attribute_id);
                values.insert(symbolic, value);
            }
            out.push(ReplayAttributeScope {
                scope: scope.clone(),
                values,
            });
        }
        out
    }

    pub(crate) fn from_value(value: Value) -> Result<Self, DecodeError> {
        let map = ReplayValueReader::as_object(&value)?;
        let mut scopes = BTreeMap::new();
        if let Some(scope_map) = map.get("scopes").and_then(ReplayValueReader::as_object_opt) {
            for (scope_key, scope_value) in scope_map {
                let attrs = ReplayValueReader::as_object(scope_value)?;
                let mut entries = BTreeMap::new();
                for (attr_key, attr_value) in attrs {
                    let items = ReplayValueReader::as_array(attr_value)?;
                    let parsed_items = items
                        .iter()
                        .map(ReplayAttributeValue::from_value)
                        .collect::<Result<Vec<_>, _>>()?;
                    entries.insert(attr_key.clone(), parsed_items);
                }
                scopes.insert(scope_key.clone(), entries);
            }
        }

        Ok(Self {
            source: ReplayValueReader::get_u8(map, "source").unwrap_or_default(),
            mapNamespace: ReplayValueReader::get_u32(map, "mapNamespace").unwrap_or_default(),
            count: ReplayValueReader::get_u32(map, "count").unwrap_or_default(),
            scopes,
        })
    }
}

impl ReplayAttributeValue {
    fn from_value(value: &Value) -> Result<Self, DecodeError> {
        let map = ReplayValueReader::as_object(value)?;
        Ok(Self {
            namespace: ReplayValueReader::get_u32(map, "namespace").unwrap_or_default(),
            attrid: ReplayValueReader::get_u32(map, "attrid").unwrap_or_default(),
            scope: ReplayValueReader::get_u8(map, "scope").unwrap_or_default(),
            value: map
                .get("value")
                .and_then(ReplayValueReader::value_as_bytes)
                .unwrap_or_default(),
        })
    }
}

struct ReplayValueReader;

impl ReplayValueReader {
    fn parse_attribute_name_map() -> HashMap<u32, String> {
        let mut attrs = HashMap::new();
        for line in include_str!("../protocols/attributes.py").lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("###") || trimmed.starts_with('#') || trimmed.is_empty() {
                continue;
            }
            let Some((name, value)) = trimmed.split_once('=') else {
                continue;
            };
            let name = name.trim();
            if name.is_empty()
                || name.chars().next().is_none_or(|c| c.is_ascii_digit())
                || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                continue;
            }

            if let Ok(id) = value.trim().parse::<u32>() {
                attrs.insert(id, name.to_ascii_lowercase());
            }
        }
        attrs
    }

    fn attribute_id_to_name(names: &HashMap<u32, String>, attribute_id: &str) -> String {
        attribute_id
            .parse::<u32>()
            .ok()
            .and_then(|id| names.get(&id).cloned())
            .unwrap_or_else(|| format!("unknown_{attribute_id}"))
    }

    fn parse_cache_handles(values: &[Value]) -> Vec<String> {
        values
            .iter()
            .filter_map(|value| match value {
                Value::Bytes(bytes) => ReplayFormat::cache_handle_uri(bytes),
                Value::String(text) if !text.is_empty() => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    fn as_object(value: &Value) -> Result<&BTreeMap<String, Value>, DecodeError> {
        ReplayValueReader::as_object_opt(value)
            .ok_or_else(|| DecodeError::Corrupted("expected object".into()))
    }

    fn as_object_opt(value: &Value) -> Option<&BTreeMap<String, Value>> {
        match value {
            Value::Object(map) => Some(map),
            _ => None,
        }
    }

    fn as_array(value: &Value) -> Result<&[Value], DecodeError> {
        ReplayValueReader::as_array_opt(value)
            .ok_or_else(|| DecodeError::Corrupted("expected array".into()))
    }

    fn as_array_opt(value: &Value) -> Option<&[Value]> {
        match value {
            Value::Array(values) => Some(values),
            _ => None,
        }
    }

    fn get_i64(map: &BTreeMap<String, Value>, key: &str) -> Option<i64> {
        map.get(key).and_then(ReplayValueReader::value_as_i64)
    }

    fn get_u32(map: &BTreeMap<String, Value>, key: &str) -> Option<u32> {
        ReplayValueReader::get_i64(map, key).and_then(|value| u32::try_from(value).ok())
    }

    fn get_u64(map: &BTreeMap<String, Value>, key: &str) -> Option<u64> {
        ReplayValueReader::get_i64(map, key).and_then(|value| u64::try_from(value).ok())
    }

    fn get_u8(map: &BTreeMap<String, Value>, key: &str) -> Option<u8> {
        ReplayValueReader::get_i64(map, key).and_then(|value| u8::try_from(value).ok())
    }

    fn get_bool(map: &BTreeMap<String, Value>, key: &str) -> Option<bool> {
        map.get(key).and_then(ReplayValueReader::value_as_bool)
    }

    fn get_string(map: &BTreeMap<String, Value>, key: &str) -> Option<String> {
        map.get(key).and_then(ReplayValueReader::value_as_string)
    }

    fn value_as_i64(value: &Value) -> Option<i64> {
        value
            .as_i128()
            .and_then(|value| i64::try_from(value).ok())
            .or_else(|| match value {
                Value::Float(number) => Some(*number as i64),
                Value::String(text) => text.parse::<i64>().ok(),
                _ => None,
            })
    }

    fn value_as_bool(value: &Value) -> Option<bool> {
        match value {
            Value::Bool(flag) => Some(*flag),
            Value::Int(value) => Some(*value != 0),
            Value::Float(value) => Some(*value != 0.0),
            Value::String(text) if text.eq_ignore_ascii_case("true") || text == "1" => Some(true),
            Value::String(text) if text.eq_ignore_ascii_case("false") || text == "0" => Some(false),
            _ => None,
        }
    }

    fn value_as_string(value: &Value) -> Option<String> {
        match value {
            Value::String(text) => Some(text.clone()),
            Value::Bytes(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
            Value::Int(number) => Some(number.to_string()),
            Value::Float(number) => Some(number.to_string()),
            _ => None,
        }
    }

    fn value_as_bytes(value: &Value) -> Option<Vec<u8>> {
        match value {
            Value::Bytes(bytes) => Some(bytes.clone()),
            _ => None,
        }
    }
}
