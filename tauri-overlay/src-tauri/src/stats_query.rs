use s2coop_analyzer::dictionary_data::Sc2DictionaryData;
use std::collections::HashSet;

use crate::{
    ReplayAnalysis, ReplayAnalysisOps, ReplayCacheReadScope, ReplayCacheStatsDifficultyExclusion,
    ReplayCacheStatsQuery, ReplayInfo, TauriOverlayOps,
};

pub struct StatsQuery {
    include_mutations: bool,
    include_normal_games: bool,
    include_wins: bool,
    include_losses: bool,
    include_both_main: bool,
    include_sub_15: bool,
    include_over_15: bool,
    include_ally_sub_15: bool,
    include_ally_over_15: bool,
    include_main_normal_mastery: bool,
    include_main_abnormal_mastery: bool,
    include_ally_normal_mastery: bool,
    include_ally_abnormal_mastery: bool,
    show_all: bool,
    min_length_seconds: u64,
    max_length_seconds: u64,
    min_date_seconds: Option<u64>,
    max_date_seconds: Option<u64>,
    player_filter: String,
    difficulty_exclusions: Vec<ReplayCacheStatsDifficultyExclusion>,
    region_exclusions: HashSet<String>,
}

impl StatsQuery {
    pub fn from_path(path: &str) -> Self {
        let include_wins = Self::parse_query_value(path, "include_wins")
            .map(|_| Self::parse_query_bool(path, "include_wins", true))
            .unwrap_or(true);
        let include_losses = Self::parse_query_value(path, "include_losses")
            .map(|_| Self::parse_query_bool(path, "include_losses", true))
            .unwrap_or_else(|| !Self::parse_query_bool(path, "wins_only", false));
        let min_length_seconds = Self::parse_query_i64(path, "minlength")
            .and_then(|value| u64::try_from(value.max(0)).ok())
            .unwrap_or(0)
            .saturating_mul(60);
        let max_length_seconds = Self::parse_query_i64(path, "maxlength")
            .and_then(|value| u64::try_from(value.max(0)).ok())
            .unwrap_or(0)
            .saturating_mul(60);
        let difficulty_exclusions = Self::parse_query_csv(path, "difficulty_filter")
            .into_iter()
            .filter_map(|value| ReplayCacheStatsDifficultyExclusion::from_query_value(&value))
            .collect();
        let region_exclusions = Self::parse_query_csv(path, "region_filter")
            .into_iter()
            .map(|value| value.to_ascii_uppercase())
            .collect();

        Self {
            include_mutations: Self::parse_query_bool(path, "include_mutations", true),
            include_normal_games: Self::parse_query_bool(path, "include_normal_games", true),
            include_wins,
            include_losses,
            include_both_main: Self::parse_query_bool(path, "include_both_main", true),
            include_sub_15: Self::parse_query_bool(path, "sub_15", true),
            include_over_15: Self::parse_query_bool(path, "over_15", true),
            include_ally_sub_15: Self::parse_query_bool(path, "ally_sub_15", true),
            include_ally_over_15: Self::parse_query_bool(path, "ally_over_15", true),
            include_main_normal_mastery: Self::parse_query_bool(path, "main_normal_mastery", true),
            include_main_abnormal_mastery: Self::parse_query_bool(
                path,
                "main_abnormal_mastery",
                true,
            ),
            include_ally_normal_mastery: Self::parse_query_bool(path, "ally_normal_mastery", true),
            include_ally_abnormal_mastery: Self::parse_query_bool(
                path,
                "ally_abnormal_mastery",
                true,
            ),
            show_all: Self::parse_query_bool(path, "show_all", true),
            min_length_seconds,
            max_length_seconds,
            min_date_seconds: Self::query_date_boundary_seconds(path, "mindate"),
            max_date_seconds: Self::query_date_boundary_seconds(path, "maxdate"),
            player_filter: Self::parse_query_value(path, "player")
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase(),
            difficulty_exclusions,
            region_exclusions,
        }
    }

    pub fn to_cache_query(
        &self,
        scope: ReplayCacheReadScope,
        limit: usize,
        main_handles: &HashSet<String>,
        current_replay_files: &HashSet<String>,
    ) -> ReplayCacheStatsQuery {
        let main_handle_keys = main_handles
            .iter()
            .map(|handle| ReplayAnalysis::normalized_handle_key(handle))
            .filter(|handle| !handle.is_empty())
            .collect::<Vec<_>>();
        let mut region_exclusions = self.region_exclusions.iter().cloned().collect::<Vec<_>>();
        region_exclusions.sort();
        let mut query = ReplayCacheStatsQuery::new(scope, limit)
            .with_mutation_filters(self.include_mutations, self.include_normal_games)
            .with_result_filters(self.include_wins, self.include_losses)
            .with_length_seconds(self.min_length_seconds, self.max_length_seconds)
            .with_date_seconds(self.min_date_seconds, self.max_date_seconds)
            .with_player_filter(self.player_filter.clone())
            .with_difficulty_exclusions(self.difficulty_exclusions.clone())
            .with_region_exclusions(region_exclusions)
            .with_commander_level_filters(
                self.include_sub_15,
                self.include_over_15,
                self.include_ally_sub_15,
                self.include_ally_over_15,
            )
            .with_mastery_filters(
                self.include_main_normal_mastery,
                self.include_main_abnormal_mastery,
                self.include_ally_normal_mastery,
                self.include_ally_abnormal_mastery,
            )
            .with_main_identity_filters(self.include_both_main, main_handle_keys);

        if !self.show_all {
            let mut files = current_replay_files.iter().cloned().collect::<Vec<_>>();
            files.sort();
            query = query.with_current_replay_files(files);
        }

        query
    }

    pub fn matches_replay(
        &self,
        replay: &ReplayInfo,
        main_handles: &HashSet<String>,
        dictionary: &Sc2DictionaryData,
    ) -> bool {
        if replay.result == "Unparsed" {
            return false;
        }
        if dictionary.canonicalize_coop_map_id(&replay.map).is_none() {
            return false;
        }

        if !self.include_mutations && replay.extension {
            return false;
        }
        if !self.include_normal_games && !replay.extension {
            return false;
        }
        let Some(is_victory) = TauriOverlayOps::result_is_victory(&replay.result) else {
            return false;
        };
        if !self.include_wins && is_victory {
            return false;
        }
        if !self.include_losses && !is_victory {
            return false;
        }

        if self.min_length_seconds > 0 && replay.accurate_length < self.min_length_seconds as f64 {
            return false;
        }
        if self.max_length_seconds > 0 && replay.accurate_length > self.max_length_seconds as f64 {
            return false;
        }

        let replay_date_seconds = replay.date_seconds_for_filter();
        if let Some(min_date) = self.min_date_seconds
            && replay_date_seconds <= min_date
        {
            return false;
        }
        if let Some(max_date) = self.max_date_seconds
            && replay_date_seconds >= max_date
        {
            return false;
        }

        if !self.include_sub_15 && replay.main_commander_level() < 15 {
            return false;
        }
        if !self.include_over_15 && replay.main_commander_level() >= 15 {
            return false;
        }
        if !self.include_ally_sub_15 && replay.ally_commander_level() < 15 {
            return false;
        }
        if !self.include_ally_over_15 && replay.ally_commander_level() >= 15 {
            return false;
        }
        let main_mastery_points =
            ReplayAnalysisOps::mastery_points_invested(replay.main_masteries());
        let ally_mastery_points =
            ReplayAnalysisOps::mastery_points_invested(replay.ally_masteries());
        if !self.include_main_normal_mastery && main_mastery_points <= 90 {
            return false;
        }
        if !self.include_main_abnormal_mastery && main_mastery_points > 90 {
            return false;
        }
        if !self.include_ally_normal_mastery && ally_mastery_points <= 90 {
            return false;
        }
        if !self.include_ally_abnormal_mastery && ally_mastery_points > 90 {
            return false;
        }

        if !main_handles.is_empty() && !self.include_both_main {
            let p1_is_main = main_handles.contains(&ReplayAnalysis::normalized_handle_key(
                &replay.main().handle,
            ));
            let p2_is_main = main_handles.contains(&ReplayAnalysis::normalized_handle_key(
                &replay.ally().handle,
            ));
            if p1_is_main && p2_is_main {
                return false;
            }
        }

        if !self.player_filter.is_empty() {
            let p1 = replay.main().name.to_ascii_lowercase();
            let p2 = replay.ally().name.to_ascii_lowercase();
            if !ReplayAnalysisOps::wildcard_match(&self.player_filter, &p1)
                && !ReplayAnalysisOps::wildcard_match(&self.player_filter, &p2)
            {
                return false;
            }
        }

        for exclusion in &self.difficulty_exclusions {
            if let Some(bplus) = exclusion.brutal_plus_level() {
                if replay.brutal_plus == u64::try_from(bplus).unwrap_or(0) {
                    return false;
                }
                continue;
            }

            if replay.brutal_plus > 0 && exclusion.is_brutal_label() {
                continue;
            }

            if let Some(label) = exclusion.difficulty_label()
                && replay.difficulty.contains(label)
            {
                return false;
            }
        }

        if !self.region_exclusions.is_empty() {
            let region = TauriOverlayOps::infer_region_from_handle(&replay.main().handle)
                .or_else(|| TauriOverlayOps::infer_region_from_handle(&replay.ally().handle))
                .unwrap_or_else(|| "Unknown".to_string())
                .to_ascii_uppercase();
            if !matches!(region.as_str(), "NA" | "EU" | "KR" | "CN" | "PTR") {
                return false;
            }
            if self.region_exclusions.contains(&region) {
                return false;
            }
        }

        true
    }

    fn query_date_boundary_seconds(path: &str, key: &str) -> Option<u64> {
        let value = Self::parse_query_value(path, key)?;
        ReplayAnalysisOps::parse_replay_timestamp_seconds(&value)
    }

    fn parse_query_i64(path: &str, key: &str) -> Option<i64> {
        let query = path.split('?').nth(1)?;
        for pair in query.split('&') {
            let mut parts = pair.splitn(2, '=');
            let parsed_key = parts.next()?;
            if parsed_key != key {
                continue;
            }
            let value = parts.next()?;
            if let Ok(number) = value.parse::<i64>() {
                return Some(number);
            }
        }
        None
    }

    fn query_hex_value(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    }

    fn decode_query_component(value: &str) -> String {
        let bytes = value.as_bytes();
        let mut output = Vec::with_capacity(bytes.len());
        let mut index = 0usize;
        while index < bytes.len() {
            match bytes[index] {
                b'+' => {
                    output.push(b' ');
                    index += 1;
                }
                b'%' if index + 2 < bytes.len() => {
                    let high = Self::query_hex_value(bytes[index + 1]);
                    let low = Self::query_hex_value(bytes[index + 2]);
                    if let (Some(high), Some(low)) = (high, low) {
                        output.push((high << 4) | low);
                        index += 3;
                    } else {
                        output.push(bytes[index]);
                        index += 1;
                    }
                }
                byte => {
                    output.push(byte);
                    index += 1;
                }
            }
        }
        String::from_utf8_lossy(&output).into_owned()
    }

    fn parse_query_value(path: &str, key: &str) -> Option<String> {
        let query = path.split('?').nth(1)?;
        for pair in query.split('&') {
            let mut parts = pair.splitn(2, '=');
            let parsed_key = parts.next()?;
            if parsed_key != key {
                continue;
            }
            let value = parts.next().unwrap_or("");
            return Some(Self::decode_query_component(value));
        }
        None
    }

    fn parse_query_bool(path: &str, key: &str, default: bool) -> bool {
        match Self::parse_query_value(path, key)
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "true" | "1" | "yes" | "on" => true,
            "false" | "0" | "no" | "off" => false,
            _ => default,
        }
    }

    fn parse_query_csv(path: &str, key: &str) -> Vec<String> {
        Self::parse_query_value(path, key)
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    }
}
