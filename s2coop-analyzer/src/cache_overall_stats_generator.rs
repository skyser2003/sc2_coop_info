use crate::tauri_replay_analysis_impl::ParsedReplayMessage;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;

pub use crate::detailed_replay_analysis::{ProtocolBuildValue, ReplayBuildInfo};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum CacheNumericValue {
    Integer(u64),
    Float(f64),
}

pub type ReplayMessage = ParsedReplayMessage;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlayerStatsSeries {
    pub name: String,
    pub supply: Vec<f64>,
    pub mining: Vec<f64>,
    pub army: Vec<f64>,
    pub killed: Vec<f64>,
    #[serde(skip, default)]
    pub army_force_float_indices: BTreeSet<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum CacheCountValue {
    Count(i64),
    Hidden(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CacheUnitStats(pub CacheCountValue, pub CacheCountValue, pub i64, pub f64);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CachePlayerStatsSeries {
    pub name: String,
    pub supply: Vec<f64>,
    pub mining: Vec<f64>,
    pub army: Vec<CacheStatValue>,
    pub killed: Vec<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum CacheStatValue {
    Integer(u64),
    Float(f64),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum CacheIconValue {
    Count(u64),
    Order(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CachePlayer {
    pub pid: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apm: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commander: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commander_level: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commander_mastery_level: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icons: Option<BTreeMap<String, CacheIconValue>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kills: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub masteries: Option<[u32; 6]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observer: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prestige: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prestige_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub race: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub units: Option<BTreeMap<String, CacheUnitStats>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CacheReplayEntry {
    pub accurate_length: CacheNumericValue,
    pub amon_units: Option<BTreeMap<String, CacheUnitStats>>,
    pub bonus: Option<Vec<String>>,
    pub brutal_plus: u32,
    pub build: ReplayBuildInfo,
    pub comp: Option<String>,
    pub date: String,
    pub difficulty: (String, String),
    pub enemy_race: Option<String>,
    pub ext_difficulty: String,
    pub extension: bool,
    pub file: String,
    pub form_alength: String,
    pub detailed_analysis: bool,
    pub hash: String,
    pub length: u64,
    pub map_name: String,
    pub messages: Vec<ReplayMessage>,
    pub mutators: Vec<String>,
    pub player_stats: Option<BTreeMap<u8, CachePlayerStatsSeries>>,
    pub players: Vec<CachePlayer>,
    pub region: String,
    pub result: String,
    pub weekly: bool,
}

#[derive(Debug, Error)]
pub enum PrettyCacheError {
    #[error("failed to read cache file '{0}': {1}")]
    ReadFailed(PathBuf, #[source] io::Error),
    #[error("failed to parse cache json '{0}': {1}")]
    ParseFailed(PathBuf, #[source] serde_json::Error),
    #[error("failed to serialize pretty cache json '{0}': {1}")]
    SerializeFailed(PathBuf, #[source] serde_json::Error),
    #[error("failed to write pretty cache file '{0}': {1}")]
    WriteFailed(PathBuf, #[source] io::Error),
}

pub fn pretty_output_path(path: &Path) -> PathBuf {
    let extension = path.extension();
    let file_name = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("cache_overall_stats");

    path.with_file_name(format!(
        "{file_name}_pretty.{}",
        extension.and_then(|s| s.to_str()).unwrap_or("json")
    ))
}

pub fn write_pretty_cache_file(
    minified_path: &Path,
    pretty_path: Option<&Path>,
) -> Result<PathBuf, PrettyCacheError> {
    let payload = fs::read(minified_path)
        .map_err(|error| PrettyCacheError::ReadFailed(minified_path.to_path_buf(), error))?;
    let parsed: JsonValue = serde_json::from_slice(&payload)
        .map_err(|error| PrettyCacheError::ParseFailed(minified_path.to_path_buf(), error))?;
    let target_path = pretty_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| pretty_output_path(minified_path));
    let pretty_text = serde_json::to_string_pretty(&parsed)
        .map_err(|error| PrettyCacheError::SerializeFailed(minified_path.to_path_buf(), error))?;
    fs::write(&target_path, format!("{pretty_text}\n"))
        .map_err(|error| PrettyCacheError::WriteFailed(target_path.clone(), error))?;
    Ok(target_path)
}
