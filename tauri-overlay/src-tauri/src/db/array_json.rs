use super::core::ReplayCacheDbError;
use s2coop_analyzer::cache_overall_stats_generator::CacheStatValue;
use serde::{Serialize, de::DeserializeOwned};

pub struct ReplayCacheArrayJson;

impl ReplayCacheArrayJson {
    pub fn encode_f64(values: &[f64]) -> Result<String, ReplayCacheDbError> {
        Self::encode_json_array("f64 array", values)
    }

    pub fn decode_f64(text: &str) -> Result<Vec<f64>, ReplayCacheDbError> {
        Self::decode_json_array("f64 array", text)
    }

    pub fn encode_strings(values: &[String]) -> Result<String, ReplayCacheDbError> {
        Self::encode_json_array("string array", values)
    }

    pub fn decode_strings(text: &str) -> Result<Vec<String>, ReplayCacheDbError> {
        Self::decode_json_array("string array", text)
    }

    pub fn encode_u64(values: &[u64]) -> Result<String, ReplayCacheDbError> {
        Self::encode_json_array("u64 array", values)
    }

    pub fn decode_u64(text: &str) -> Result<Vec<u64>, ReplayCacheDbError> {
        Self::decode_json_array("u64 array", text)
    }

    pub fn encode_u32(values: &[u32]) -> Result<String, ReplayCacheDbError> {
        Self::encode_json_array("u32 array", values)
    }

    pub fn decode_u32(text: &str) -> Result<Vec<u32>, ReplayCacheDbError> {
        Self::decode_json_array("u32 array", text)
    }

    pub fn encode_stat_values(values: &[CacheStatValue]) -> Result<String, ReplayCacheDbError> {
        Self::encode_json_array("stat value array", values)
    }

    pub fn decode_stat_values(text: &str) -> Result<Vec<CacheStatValue>, ReplayCacheDbError> {
        Self::decode_json_array("stat value array", text)
    }

    fn encode_json_array<T>(
        context: &'static str,
        values: &[T],
    ) -> Result<String, ReplayCacheDbError>
    where
        T: Serialize,
    {
        serde_json::to_string(values)
            .map_err(|source| ReplayCacheDbError::JsonArray { context, source })
    }

    fn decode_json_array<T>(context: &'static str, text: &str) -> Result<Vec<T>, ReplayCacheDbError>
    where
        T: DeserializeOwned,
    {
        serde_json::from_str::<Vec<T>>(text)
            .map_err(|source| ReplayCacheDbError::JsonArray { context, source })
    }
}
