use chrono::{Local, LocalResult, TimeZone, Utc};
use rusqlite::{Connection, ErrorCode, Row};
use s2coop_analyzer::cache_overall_stats_generator::{
    CacheCountValue, CacheNumericValue, CacheReplayEntry, ProtocolBuildValue,
};
use s2coop_analyzer::detailed_replay_analysis::{CacheEntrySink, CacheEntrySinkError};
use serde_json::Value;
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::array_json::ReplayCacheArrayJson;
use crate::replay_analysis::ReplayAnalysisOps;

const CURRENT_SCHEMA_VERSION: i32 = 1;
const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_secs(60);
const SQLITE_LOCK_RETRY_WINDOW: Duration = Duration::from_secs(120);
const SQLITE_LOCK_RETRY_DELAY: Duration = Duration::from_millis(100);

#[derive(Debug)]
pub enum ReplayCacheDbError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    JsonArray {
        context: &'static str,
        source: serde_json::Error,
    },
    Sqlite {
        path: PathBuf,
        source: rusqlite::Error,
    },
    UnsupportedSchema {
        path: PathBuf,
        version: i32,
        supported: i32,
    },
}

impl Display for ReplayCacheDbError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(
                    formatter,
                    "cache database io error '{}': {source}",
                    path.display()
                )
            }
            Self::Json { path, source } => {
                write!(
                    formatter,
                    "cache database json import error '{}': {source}",
                    path.display()
                )
            }
            Self::JsonArray { context, source } => {
                write!(
                    formatter,
                    "cache database json array error '{context}': {source}"
                )
            }
            Self::Sqlite { path, source } => {
                write!(
                    formatter,
                    "cache database sqlite error '{}': {source}",
                    path.display()
                )
            }
            Self::UnsupportedSchema {
                path,
                version,
                supported,
            } => {
                write!(
                    formatter,
                    "cache database '{}' has unsupported schema version {version}; supported version is {supported}",
                    path.display()
                )
            }
        }
    }
}

impl Error for ReplayCacheDbError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Json { source, .. } => Some(source),
            Self::JsonArray { source, .. } => Some(source),
            Self::Sqlite { source, .. } => Some(source),
            Self::UnsupportedSchema { .. } => None,
        }
    }
}

impl ReplayCacheDbError {
    pub(super) fn is_sqlite_lock(&self) -> bool {
        match self {
            Self::Sqlite { source, .. } => matches!(
                source.sqlite_error_code(),
                Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked)
            ),
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayCacheReadScope {
    All,
    DetailedOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReplayCacheEntryQuery {
    scope: ReplayCacheReadScope,
    limit: usize,
}

impl ReplayCacheEntryQuery {
    pub fn all(limit: usize) -> Self {
        Self {
            scope: ReplayCacheReadScope::All,
            limit,
        }
    }

    pub fn detailed_only(limit: usize) -> Self {
        Self {
            scope: ReplayCacheReadScope::DetailedOnly,
            limit,
        }
    }

    fn scope(&self) -> ReplayCacheReadScope {
        self.scope
    }

    fn limit(&self) -> usize {
        self.limit
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReplayCacheStatsDifficultyExclusion {
    Casual,
    Normal,
    Hard,
    Brutal,
    BrutalPlus1,
    BrutalPlus2,
    BrutalPlus3,
    BrutalPlus4,
    BrutalPlus5,
    BrutalPlus6,
    Other(String),
}

impl ReplayCacheStatsDifficultyExclusion {
    pub fn from_query_value(value: &str) -> Option<Self> {
        let trimmed = value.trim();
        match trimmed {
            "Casual" => Some(Self::Casual),
            "Normal" => Some(Self::Normal),
            "Hard" => Some(Self::Hard),
            "Brutal" => Some(Self::Brutal),
            "1" | "BrutalPlus1" => Some(Self::BrutalPlus1),
            "2" | "BrutalPlus2" => Some(Self::BrutalPlus2),
            "3" | "BrutalPlus3" => Some(Self::BrutalPlus3),
            "4" | "BrutalPlus4" => Some(Self::BrutalPlus4),
            "5" | "BrutalPlus5" => Some(Self::BrutalPlus5),
            "6" | "BrutalPlus6" => Some(Self::BrutalPlus6),
            "" => None,
            _ => Some(Self::Other(trimmed.to_string())),
        }
    }

    pub fn brutal_plus_level(&self) -> Option<i64> {
        match self {
            Self::BrutalPlus1 => Some(1),
            Self::BrutalPlus2 => Some(2),
            Self::BrutalPlus3 => Some(3),
            Self::BrutalPlus4 => Some(4),
            Self::BrutalPlus5 => Some(5),
            Self::BrutalPlus6 => Some(6),
            _ => None,
        }
    }

    pub fn difficulty_label(&self) -> Option<&str> {
        match self {
            Self::Casual => Some("Casual"),
            Self::Normal => Some("Normal"),
            Self::Hard => Some("Hard"),
            Self::Brutal => Some("Brutal"),
            Self::Other(value) => Some(value.as_str()),
            Self::BrutalPlus1
            | Self::BrutalPlus2
            | Self::BrutalPlus3
            | Self::BrutalPlus4
            | Self::BrutalPlus5
            | Self::BrutalPlus6 => None,
        }
    }

    pub fn is_brutal_label(&self) -> bool {
        matches!(self, Self::Brutal)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayCacheStatsQuery {
    scope: ReplayCacheReadScope,
    limit: usize,
    include_mutations: bool,
    include_normal_games: bool,
    include_wins: bool,
    include_losses: bool,
    min_length_seconds: u64,
    max_length_seconds: u64,
    min_date_seconds: Option<u64>,
    max_date_seconds: Option<u64>,
    player_filter: String,
    difficulty_exclusions: Vec<ReplayCacheStatsDifficultyExclusion>,
    region_exclusions: Vec<String>,
    current_replay_files: Vec<String>,
    restrict_to_current_replay_files: bool,
    include_sub_15: bool,
    include_over_15: bool,
    include_ally_sub_15: bool,
    include_ally_over_15: bool,
    include_main_normal_mastery: bool,
    include_main_abnormal_mastery: bool,
    include_ally_normal_mastery: bool,
    include_ally_abnormal_mastery: bool,
    include_both_main: bool,
    main_handle_keys: Vec<String>,
}

impl ReplayCacheStatsQuery {
    pub fn new(scope: ReplayCacheReadScope, limit: usize) -> Self {
        Self {
            scope,
            limit,
            include_mutations: true,
            include_normal_games: true,
            include_wins: true,
            include_losses: true,
            min_length_seconds: 0,
            max_length_seconds: 0,
            min_date_seconds: None,
            max_date_seconds: None,
            player_filter: String::new(),
            difficulty_exclusions: Vec::new(),
            region_exclusions: Vec::new(),
            current_replay_files: Vec::new(),
            restrict_to_current_replay_files: false,
            include_sub_15: true,
            include_over_15: true,
            include_ally_sub_15: true,
            include_ally_over_15: true,
            include_main_normal_mastery: true,
            include_main_abnormal_mastery: true,
            include_ally_normal_mastery: true,
            include_ally_abnormal_mastery: true,
            include_both_main: true,
            main_handle_keys: Vec::new(),
        }
    }

    pub fn with_scope(mut self, scope: ReplayCacheReadScope) -> Self {
        self.scope = scope;
        self
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    pub fn with_mutation_filters(
        mut self,
        include_mutations: bool,
        include_normal_games: bool,
    ) -> Self {
        self.include_mutations = include_mutations;
        self.include_normal_games = include_normal_games;
        self
    }

    pub fn with_result_filters(mut self, include_wins: bool, include_losses: bool) -> Self {
        self.include_wins = include_wins;
        self.include_losses = include_losses;
        self
    }

    pub fn with_length_seconds(mut self, min_length_seconds: u64, max_length_seconds: u64) -> Self {
        self.min_length_seconds = min_length_seconds;
        self.max_length_seconds = max_length_seconds;
        self
    }

    pub fn with_date_seconds(
        mut self,
        min_date_seconds: Option<u64>,
        max_date_seconds: Option<u64>,
    ) -> Self {
        self.min_date_seconds = min_date_seconds;
        self.max_date_seconds = max_date_seconds;
        self
    }

    pub fn with_player_filter(mut self, player_filter: String) -> Self {
        self.player_filter = player_filter;
        self
    }

    pub fn with_difficulty_exclusions(
        mut self,
        difficulty_exclusions: Vec<ReplayCacheStatsDifficultyExclusion>,
    ) -> Self {
        self.difficulty_exclusions = difficulty_exclusions;
        self
    }

    pub fn with_region_exclusions(mut self, region_exclusions: Vec<String>) -> Self {
        self.region_exclusions = region_exclusions;
        self
    }

    pub fn with_current_replay_files(mut self, current_replay_files: Vec<String>) -> Self {
        self.current_replay_files = current_replay_files;
        self.restrict_to_current_replay_files = true;
        self
    }

    pub fn with_commander_level_filters(
        mut self,
        include_sub_15: bool,
        include_over_15: bool,
        include_ally_sub_15: bool,
        include_ally_over_15: bool,
    ) -> Self {
        self.include_sub_15 = include_sub_15;
        self.include_over_15 = include_over_15;
        self.include_ally_sub_15 = include_ally_sub_15;
        self.include_ally_over_15 = include_ally_over_15;
        self
    }

    pub fn with_mastery_filters(
        mut self,
        include_main_normal_mastery: bool,
        include_main_abnormal_mastery: bool,
        include_ally_normal_mastery: bool,
        include_ally_abnormal_mastery: bool,
    ) -> Self {
        self.include_main_normal_mastery = include_main_normal_mastery;
        self.include_main_abnormal_mastery = include_main_abnormal_mastery;
        self.include_ally_normal_mastery = include_ally_normal_mastery;
        self.include_ally_abnormal_mastery = include_ally_abnormal_mastery;
        self
    }

    pub fn with_main_identity_filters(
        mut self,
        include_both_main: bool,
        main_handle_keys: Vec<String>,
    ) -> Self {
        self.include_both_main = include_both_main;
        self.main_handle_keys = main_handle_keys;
        self
    }

    pub(super) fn scope(&self) -> ReplayCacheReadScope {
        self.scope
    }

    pub(super) fn limit(&self) -> usize {
        self.limit
    }

    pub(super) fn include_mutations(&self) -> bool {
        self.include_mutations
    }

    pub(super) fn include_normal_games(&self) -> bool {
        self.include_normal_games
    }

    pub(super) fn include_wins(&self) -> bool {
        self.include_wins
    }

    pub(super) fn include_losses(&self) -> bool {
        self.include_losses
    }

    pub(super) fn min_length_seconds(&self) -> u64 {
        self.min_length_seconds
    }

    pub(super) fn max_length_seconds(&self) -> u64 {
        self.max_length_seconds
    }

    pub(super) fn min_date_seconds(&self) -> Option<u64> {
        self.min_date_seconds
    }

    pub(super) fn max_date_seconds(&self) -> Option<u64> {
        self.max_date_seconds
    }

    pub(super) fn player_filter(&self) -> &str {
        &self.player_filter
    }

    pub(super) fn difficulty_exclusions(&self) -> &[ReplayCacheStatsDifficultyExclusion] {
        &self.difficulty_exclusions
    }

    pub(super) fn region_exclusions(&self) -> &[String] {
        &self.region_exclusions
    }

    pub(super) fn current_replay_files(&self) -> &[String] {
        &self.current_replay_files
    }

    pub(super) fn restrict_to_current_replay_files(&self) -> bool {
        self.restrict_to_current_replay_files
    }

    pub(super) fn include_sub_15(&self) -> bool {
        self.include_sub_15
    }

    pub(super) fn include_over_15(&self) -> bool {
        self.include_over_15
    }

    pub(super) fn include_ally_sub_15(&self) -> bool {
        self.include_ally_sub_15
    }

    pub(super) fn include_ally_over_15(&self) -> bool {
        self.include_ally_over_15
    }

    pub(super) fn include_main_normal_mastery(&self) -> bool {
        self.include_main_normal_mastery
    }

    pub(super) fn include_main_abnormal_mastery(&self) -> bool {
        self.include_main_abnormal_mastery
    }

    pub(super) fn include_ally_normal_mastery(&self) -> bool {
        self.include_ally_normal_mastery
    }

    pub(super) fn include_ally_abnormal_mastery(&self) -> bool {
        self.include_ally_abnormal_mastery
    }

    pub(super) fn include_both_main(&self) -> bool {
        self.include_both_main
    }

    pub(super) fn main_handle_keys(&self) -> &[String] {
        &self.main_handle_keys
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayCacheSortDirection {
    Asc,
    Desc,
}

impl ReplayCacheSortDirection {
    pub fn from_query_value(value: Option<&str>, default: Self) -> Self {
        match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("asc") => Self::Asc,
            Some("desc") => Self::Desc,
            _ => default,
        }
    }

    pub(super) fn sql_keyword(self) -> &'static str {
        match self {
            Self::Asc => "ASC",
            Self::Desc => "DESC",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReplayCachePage {
    page: usize,
    rows_per_page: usize,
}

impl ReplayCachePage {
    pub fn new(page: usize, rows_per_page: usize) -> Self {
        Self {
            page: page.max(1),
            rows_per_page: rows_per_page.max(1),
        }
    }

    pub(super) fn offset(&self) -> usize {
        self.page
            .saturating_sub(1)
            .saturating_mul(self.rows_per_page)
    }

    pub(super) fn limit(&self) -> usize {
        self.rows_per_page
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayCachePageResult<T> {
    rows: Vec<T>,
    total_rows: usize,
}

impl<T> ReplayCachePageResult<T> {
    pub fn new(rows: Vec<T>, total_rows: usize) -> Self {
        Self { rows, total_rows }
    }

    pub fn into_rows_and_total(self) -> (Vec<T>, usize) {
        (self.rows, self.total_rows)
    }

    pub fn rows(&self) -> &[T] {
        &self.rows
    }

    pub fn total_rows(&self) -> usize {
        self.total_rows
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReplayCacheStatisticsPayload {
    analysis: Value,
    prestige_names: BTreeMap<String, crate::shared_types::LocalizedLabels>,
    games: u64,
    detailed_parsed_count: u64,
    total_valid_files: u64,
    main_players: Vec<String>,
    main_handles: Vec<String>,
}

impl ReplayCacheStatisticsPayload {
    pub(super) fn new(
        analysis: Value,
        prestige_names: BTreeMap<String, crate::shared_types::LocalizedLabels>,
        games: u64,
        detailed_parsed_count: u64,
        total_valid_files: u64,
        main_players: Vec<String>,
        main_handles: Vec<String>,
    ) -> Self {
        Self {
            analysis,
            prestige_names,
            games,
            detailed_parsed_count,
            total_valid_files,
            main_players,
            main_handles,
        }
    }

    pub fn analysis(&self) -> &Value {
        &self.analysis
    }

    pub fn prestige_names(&self) -> &BTreeMap<String, crate::shared_types::LocalizedLabels> {
        &self.prestige_names
    }

    pub fn games(&self) -> u64 {
        self.games
    }

    pub fn detailed_parsed_count(&self) -> u64 {
        self.detailed_parsed_count
    }

    pub fn total_valid_files(&self) -> u64 {
        self.total_valid_files
    }

    pub fn main_players(&self) -> &[String] {
        &self.main_players
    }

    pub fn main_handles(&self) -> &[String] {
        &self.main_handles
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayCacheGameSortKey {
    Map,
    Result,
    PlayerOne,
    PlayerTwo,
    Enemy,
    Length,
    Difficulty,
    Mutators,
    Time,
    Actions,
}

impl ReplayCacheGameSortKey {
    pub fn from_query_value(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some("map") => Self::Map,
            Some("result") => Self::Result,
            Some("p1") => Self::PlayerOne,
            Some("p2") => Self::PlayerTwo,
            Some("enemy") => Self::Enemy,
            Some("length") => Self::Length,
            Some("difficulty") => Self::Difficulty,
            Some("mutators") => Self::Mutators,
            Some("actions") => Self::Actions,
            Some("time") | None => Self::Time,
            _ => Self::Time,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayCacheDifficultyFilter {
    Casual,
    Normal,
    Hard,
    Brutal,
    BrutalPlus1,
    BrutalPlus2,
    BrutalPlus3,
    BrutalPlus4,
    BrutalPlus5,
    BrutalPlus6,
}

impl ReplayCacheDifficultyFilter {
    pub fn from_query_value(value: &str) -> Option<Self> {
        match value.trim() {
            "Casual" => Some(Self::Casual),
            "Normal" => Some(Self::Normal),
            "Hard" => Some(Self::Hard),
            "Brutal" => Some(Self::Brutal),
            "BrutalPlus1" => Some(Self::BrutalPlus1),
            "BrutalPlus2" => Some(Self::BrutalPlus2),
            "BrutalPlus3" => Some(Self::BrutalPlus3),
            "BrutalPlus4" => Some(Self::BrutalPlus4),
            "BrutalPlus5" => Some(Self::BrutalPlus5),
            "BrutalPlus6" => Some(Self::BrutalPlus6),
            _ => None,
        }
    }

    pub fn all() -> Vec<Self> {
        vec![
            Self::Casual,
            Self::Normal,
            Self::Hard,
            Self::Brutal,
            Self::BrutalPlus1,
            Self::BrutalPlus2,
            Self::BrutalPlus3,
            Self::BrutalPlus4,
            Self::BrutalPlus5,
            Self::BrutalPlus6,
        ]
    }

    pub(super) fn brutal_plus_level(self) -> Option<i64> {
        match self {
            Self::BrutalPlus1 => Some(1),
            Self::BrutalPlus2 => Some(2),
            Self::BrutalPlus3 => Some(3),
            Self::BrutalPlus4 => Some(4),
            Self::BrutalPlus5 => Some(5),
            Self::BrutalPlus6 => Some(6),
            _ => None,
        }
    }

    pub(super) fn regular_label(self) -> Option<&'static str> {
        match self {
            Self::Casual => Some("casual"),
            Self::Normal => Some("normal"),
            Self::Hard => Some("hard"),
            Self::Brutal => Some("brutal"),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayCacheGamesPageQuery {
    page: ReplayCachePage,
    search: String,
    sort_key: ReplayCacheGameSortKey,
    sort_direction: ReplayCacheSortDirection,
    difficulty_filters: Vec<ReplayCacheDifficultyFilter>,
    include_normal_games: bool,
    include_mutation_games: bool,
}

impl ReplayCacheGamesPageQuery {
    pub fn new(
        page: ReplayCachePage,
        search: String,
        sort_key: ReplayCacheGameSortKey,
        sort_direction: ReplayCacheSortDirection,
        difficulty_filters: Vec<ReplayCacheDifficultyFilter>,
        include_normal_games: bool,
        include_mutation_games: bool,
    ) -> Self {
        Self {
            page,
            search,
            sort_key,
            sort_direction,
            difficulty_filters,
            include_normal_games,
            include_mutation_games,
        }
    }

    pub(super) fn page(&self) -> ReplayCachePage {
        self.page
    }

    pub(super) fn search(&self) -> &str {
        &self.search
    }

    pub(super) fn sort_key(&self) -> ReplayCacheGameSortKey {
        self.sort_key
    }

    pub(super) fn sort_direction(&self) -> ReplayCacheSortDirection {
        self.sort_direction
    }

    pub(super) fn difficulty_filters(&self) -> &[ReplayCacheDifficultyFilter] {
        &self.difficulty_filters
    }

    pub(super) fn include_normal_games(&self) -> bool {
        self.include_normal_games
    }

    pub(super) fn include_mutation_games(&self) -> bool {
        self.include_mutation_games
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayCachePlayerSortKey {
    Handle,
    Player,
    Wins,
    Losses,
    Winrate,
    Apm,
    Commander,
    Frequency,
    Kills,
    LastSeen,
    Note,
}

impl ReplayCachePlayerSortKey {
    pub fn from_query_value(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some("handle") => Self::Handle,
            Some("player") => Self::Player,
            Some("wins") => Self::Wins,
            Some("losses") => Self::Losses,
            Some("winrate") => Self::Winrate,
            Some("apm") => Self::Apm,
            Some("commander") => Self::Commander,
            Some("frequency") => Self::Frequency,
            Some("kills") => Self::Kills,
            Some("note") => Self::Note,
            Some("last_seen") | None => Self::LastSeen,
            _ => Self::LastSeen,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayCachePlayerNote {
    handle: String,
    note: String,
}

impl ReplayCachePlayerNote {
    pub fn new(handle: String, note: String) -> Self {
        Self { handle, note }
    }

    pub(super) fn handle(&self) -> &str {
        &self.handle
    }

    pub(super) fn note(&self) -> &str {
        &self.note
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayCachePlayersPageQuery {
    page: ReplayCachePage,
    search: String,
    sort_key: ReplayCachePlayerSortKey,
    sort_direction: ReplayCacheSortDirection,
    notes: Vec<ReplayCachePlayerNote>,
}

impl ReplayCachePlayersPageQuery {
    pub fn new(
        page: ReplayCachePage,
        search: String,
        sort_key: ReplayCachePlayerSortKey,
        sort_direction: ReplayCacheSortDirection,
        notes: Vec<ReplayCachePlayerNote>,
    ) -> Self {
        Self {
            page,
            search,
            sort_key,
            sort_direction,
            notes,
        }
    }

    pub(super) fn page(&self) -> ReplayCachePage {
        self.page
    }

    pub(super) fn search(&self) -> &str {
        &self.search
    }

    pub(super) fn sort_key(&self) -> ReplayCachePlayerSortKey {
        self.sort_key
    }

    pub(super) fn sort_direction(&self) -> ReplayCacheSortDirection {
        self.sort_direction
    }

    pub(super) fn notes(&self) -> &[ReplayCachePlayerNote] {
        &self.notes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ReplayCacheTable {
    Weekly,
    Players,
    PlayerUnits,
    PlayerIcons,
    PlayerIconOrders,
    Messages,
    AmonUnits,
    PlayerStatSeries,
}

pub(super) const REPLAY_CACHE_CHILD_TABLES: [ReplayCacheTable; 8] = [
    ReplayCacheTable::Weekly,
    ReplayCacheTable::PlayerUnits,
    ReplayCacheTable::PlayerIconOrders,
    ReplayCacheTable::PlayerIcons,
    ReplayCacheTable::Messages,
    ReplayCacheTable::AmonUnits,
    ReplayCacheTable::PlayerStatSeries,
    ReplayCacheTable::Players,
];

impl ReplayCacheTable {
    pub(super) fn delete_by_replay_id_sql(self) -> &'static str {
        match self {
            Self::Players => "DELETE FROM replay_cache_players WHERE replay_id = ?1",
            Self::Weekly => "DELETE FROM replay_cache_weeklies WHERE replay_id = ?1",
            Self::PlayerUnits => "DELETE FROM replay_cache_player_units WHERE replay_id = ?1",
            Self::PlayerIcons => "DELETE FROM replay_cache_player_icons WHERE replay_id = ?1",
            Self::PlayerIconOrders => {
                "DELETE FROM replay_cache_player_icon_orders WHERE replay_id = ?1"
            }
            Self::Messages => "DELETE FROM replay_cache_messages WHERE replay_id = ?1",
            Self::AmonUnits => "DELETE FROM replay_cache_amon_units WHERE replay_id = ?1",
            Self::PlayerStatSeries => {
                "DELETE FROM replay_cache_player_stat_series WHERE replay_id = ?1"
            }
        }
    }
}

pub(super) const REPLAY_CACHE_ENTRY_RECORD_COLUMNS: &str = "
    id,
    hash,
    file,
    file_name,
    date_text,
    date_seconds,
    detailed_analysis,
    result,
    map_name,
    difficulty_p1,
    difficulty_p2,
    ext_difficulty,
    brutal_plus,
    extension,
    weekly,
    region,
    length_ingame_seconds,
    length_realtime_kind,
    length_realtime_int,
    length_realtime_float,
    form_length_realtime,
    replay_build,
    protocol_build_kind,
    protocol_build_int,
    protocol_build_text,
    comp,
    enemy_race,
    has_amon_units,
    has_bonus,
    has_player_stats,
    mutator_values,
    bonus_values,
    updated_at_seconds
";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ReplayCacheEntryRecordQuery {
    All,
    AllLimited,
    DetailedOnly,
    DetailedOnlyLimited,
}

impl ReplayCacheEntryRecordQuery {
    pub(super) fn from_entry_query(query: ReplayCacheEntryQuery) -> Self {
        match (query.scope(), query.limit()) {
            (ReplayCacheReadScope::All, 0) => Self::All,
            (ReplayCacheReadScope::All, _) => Self::AllLimited,
            (ReplayCacheReadScope::DetailedOnly, 0) => Self::DetailedOnly,
            (ReplayCacheReadScope::DetailedOnly, _) => Self::DetailedOnlyLimited,
        }
    }

    pub(super) fn limit(self, query: ReplayCacheEntryQuery) -> Option<i64> {
        match self {
            Self::All | Self::DetailedOnly => None,
            Self::AllLimited | Self::DetailedOnlyLimited => {
                Some(i64::try_from(query.limit()).unwrap_or(i64::MAX))
            }
        }
    }

    pub(super) fn sql(self) -> String {
        let filter = match self {
            Self::DetailedOnly | Self::DetailedOnlyLimited => "WHERE detailed_analysis = 1",
            Self::All | Self::AllLimited => "",
        };
        let limit = match self {
            Self::AllLimited | Self::DetailedOnlyLimited => "LIMIT ?1",
            Self::All | Self::DetailedOnly => "",
        };
        format!(
            "
            SELECT {REPLAY_CACHE_ENTRY_RECORD_COLUMNS}
            FROM replay_cache_entries
            {filter}
            ORDER BY date_seconds DESC, date_text DESC, file DESC, hash DESC
            {limit}
            "
        )
    }
}

pub(super) struct ReplayCacheEntrySql;

impl ReplayCacheEntrySql {
    pub(super) const DELETE_ALL: &'static str = "DELETE FROM replay_cache_entries";
    pub(super) const DELETE_BY_FILE_EXCEPT_HASH: &'static str =
        "DELETE FROM replay_cache_entries WHERE file = ?1 AND hash <> ?2";
    pub(super) const SELECT_ID_BY_HASH: &'static str =
        "SELECT id FROM replay_cache_entries WHERE hash = ?1";
    pub(super) const SELECT_BY_HASH: &'static str = "
        SELECT
            id,
            hash,
            file,
            file_name,
            date_text,
            date_seconds,
            detailed_analysis,
            result,
            map_name,
            difficulty_p1,
            difficulty_p2,
            ext_difficulty,
            brutal_plus,
            extension,
            weekly,
            region,
            length_ingame_seconds,
            length_realtime_kind,
            length_realtime_int,
            length_realtime_float,
            form_length_realtime,
            replay_build,
            protocol_build_kind,
            protocol_build_int,
            protocol_build_text,
            comp,
            enemy_race,
            has_amon_units,
            has_bonus,
            has_player_stats,
            mutator_values,
            bonus_values,
            updated_at_seconds
        FROM replay_cache_entries
        WHERE hash = ?1
    ";
    pub(super) const SELECT_BY_ID: &'static str = "
        SELECT
            id,
            hash,
            file,
            file_name,
            date_text,
            date_seconds,
            detailed_analysis,
            result,
            map_name,
            difficulty_p1,
            difficulty_p2,
            ext_difficulty,
            brutal_plus,
            extension,
            weekly,
            region,
            length_ingame_seconds,
            length_realtime_kind,
            length_realtime_int,
            length_realtime_float,
            form_length_realtime,
            replay_build,
            protocol_build_kind,
            protocol_build_int,
            protocol_build_text,
            comp,
            enemy_race,
            has_amon_units,
            has_bonus,
            has_player_stats,
            mutator_values,
            bonus_values,
            updated_at_seconds
        FROM replay_cache_entries
        WHERE id = ?1
    ";
    pub(super) const SELECT_IDS_PAGE: &'static str = "
        SELECT id FROM replay_cache_entries
        ORDER BY date_seconds DESC, date_text DESC, file DESC, hash DESC
        LIMIT ?1 OFFSET ?2
    ";
    pub(super) const SELECT_NEWER_IDS: &'static str = "
        SELECT id FROM replay_cache_entries
        WHERE
            date_seconds > ?1 OR
            (date_seconds = ?1 AND date_text > ?2) OR
            (date_seconds = ?1 AND date_text = ?2 AND file > ?3) OR
            (date_seconds = ?1 AND date_text = ?2 AND file = ?3 AND hash > ?4)
        ORDER BY date_seconds ASC, date_text ASC, file ASC, hash ASC
        LIMIT ?5 OFFSET ?6
    ";
    pub(super) const SELECT_OLDER_IDS: &'static str = "
        SELECT id FROM replay_cache_entries
        WHERE
            date_seconds < ?1 OR
            (date_seconds = ?1 AND date_text < ?2) OR
            (date_seconds = ?1 AND date_text = ?2 AND file < ?3) OR
            (date_seconds = ?1 AND date_text = ?2 AND file = ?3 AND hash < ?4)
        ORDER BY date_seconds DESC, date_text DESC, file DESC, hash DESC
        LIMIT ?5 OFFSET ?6
    ";
    pub(super) const SELECT_ID_BY_EXACT_FILE: &'static str =
        "SELECT id FROM replay_cache_entries WHERE file = ?1";
    pub(super) const SELECT_ID_BY_FILE_NAME: &'static str = "
        SELECT id FROM replay_cache_entries
        WHERE file_name = ?1
        ORDER BY date_seconds DESC, date_text DESC, file DESC, hash DESC
        LIMIT 1
    ";
    pub(super) const SELECT_LATEST_ID: &'static str = "
        SELECT id FROM replay_cache_entries
        ORDER BY date_seconds DESC, date_text DESC, file DESC, hash DESC
        LIMIT 1
    ";
    pub(super) const SELECT_FILES: &'static str = "SELECT file FROM replay_cache_entries";
}

#[derive(Debug)]
pub(super) struct ReplayCacheEntryRecord {
    pub(super) id: i64,
    pub(super) hash: String,
    pub(super) file: String,
    pub(super) file_name: String,
    pub(super) date_text: String,
    pub(super) date_seconds: u64,
    pub(super) detailed_analysis: bool,
    pub(super) result: String,
    pub(super) map_name: String,
    pub(super) difficulty_p1: String,
    pub(super) difficulty_p2: String,
    pub(super) ext_difficulty: String,
    pub(super) brutal_plus: u32,
    pub(super) extension: bool,
    pub(super) weekly: bool,
    pub(super) region: String,
    pub(super) length_ingame_seconds: u64,
    pub(super) length_realtime: CacheNumericValue,
    pub(super) form_length_realtime: String,
    pub(super) replay_build: u32,
    pub(super) protocol_build: ProtocolBuildValue,
    pub(super) comp: Option<String>,
    pub(super) enemy_race: Option<String>,
    pub(super) has_amon_units: bool,
    pub(super) has_bonus: bool,
    pub(super) has_player_stats: bool,
    pub(super) mutator_values: String,
    pub(super) bonus_values: String,
    pub(super) updated_at_seconds: u64,
}

impl ReplayCacheEntryRecord {
    pub(super) fn from_entry(entry: &CacheReplayEntry) -> Result<Self, ReplayCacheDbError> {
        let date_seconds =
            ReplayAnalysisOps::parse_replay_timestamp_seconds(&entry.date).unwrap_or(0);
        Ok(Self {
            id: 0,
            hash: entry.hash.clone(),
            file: entry.file.clone(),
            file_name: ReplayCacheFileName::from_replay_file(&entry.file).into_string(),
            date_text: entry.date.clone(),
            date_seconds,
            detailed_analysis: entry.detailed_analysis,
            result: entry.result.clone(),
            map_name: entry.map_name.clone(),
            difficulty_p1: entry.difficulty.0.clone(),
            difficulty_p2: entry.difficulty.1.clone(),
            ext_difficulty: entry.ext_difficulty.clone(),
            brutal_plus: entry.brutal_plus,
            extension: entry.extension,
            weekly: entry.weekly,
            region: entry.region.clone(),
            length_ingame_seconds: entry.length,
            length_realtime: entry.accurate_length.clone(),
            form_length_realtime: entry.form_alength.clone(),
            replay_build: entry.build.replay_build(),
            protocol_build: entry.build.protocol_build().clone(),
            comp: entry.comp.clone(),
            enemy_race: entry.enemy_race.clone(),
            has_amon_units: entry.amon_units.is_some(),
            has_bonus: entry.bonus.is_some(),
            has_player_stats: entry.player_stats.is_some(),
            mutator_values: ReplayCacheArrayJson::encode_strings(&entry.mutators)?,
            bonus_values: ReplayCacheArrayJson::encode_strings(
                entry.bonus.as_deref().unwrap_or(&[]),
            )?,
            updated_at_seconds: ReplayCacheDatabase::now_seconds(),
        })
    }

    pub(super) fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            hash: row.get("hash")?,
            file: row.get("file")?,
            file_name: row.get("file_name")?,
            date_text: row.get("date_text")?,
            date_seconds: Self::i64_to_u64(row.get("date_seconds")?),
            detailed_analysis: Self::i64_to_bool(row.get("detailed_analysis")?),
            result: row.get("result")?,
            map_name: row.get("map_name")?,
            difficulty_p1: row.get("difficulty_p1")?,
            difficulty_p2: row.get("difficulty_p2")?,
            ext_difficulty: row.get("ext_difficulty")?,
            brutal_plus: Self::i64_to_u32(row.get("brutal_plus")?),
            extension: Self::i64_to_bool(row.get("extension")?),
            weekly: Self::i64_to_bool(row.get("weekly")?),
            region: row.get("region")?,
            length_ingame_seconds: Self::i64_to_u64(row.get("length_ingame_seconds")?),
            length_realtime: Self::cache_numeric_from_columns(
                row.get::<_, String>("length_realtime_kind")?.as_str(),
                row.get("length_realtime_int")?,
                row.get("length_realtime_float")?,
            ),
            form_length_realtime: row.get("form_length_realtime")?,
            replay_build: Self::i64_to_u32(row.get("replay_build")?),
            protocol_build: Self::protocol_build_from_columns(
                row.get::<_, String>("protocol_build_kind")?.as_str(),
                row.get("protocol_build_int")?,
                row.get("protocol_build_text")?,
            ),
            comp: row.get("comp")?,
            enemy_race: row.get("enemy_race")?,
            has_amon_units: Self::i64_to_bool(row.get("has_amon_units")?),
            has_bonus: Self::i64_to_bool(row.get("has_bonus")?),
            has_player_stats: Self::i64_to_bool(row.get("has_player_stats")?),
            mutator_values: row.get("mutator_values")?,
            bonus_values: row.get("bonus_values")?,
            updated_at_seconds: Self::i64_to_u64(row.get("updated_at_seconds")?),
        })
    }

    pub(super) fn bool_to_i64(value: bool) -> i64 {
        if value { 1 } else { 0 }
    }

    pub(super) fn i64_to_bool(value: i64) -> bool {
        value != 0
    }

    pub(super) fn u64_to_i64(value: u64) -> i64 {
        i64::try_from(value).unwrap_or(i64::MAX)
    }

    pub(super) fn i64_to_u64(value: i64) -> u64 {
        u64::try_from(value).unwrap_or_default()
    }

    pub(super) fn i64_to_u32(value: i64) -> u32 {
        u32::try_from(value).unwrap_or_default()
    }

    pub(super) fn cache_numeric_columns(
        value: &CacheNumericValue,
    ) -> (&'static str, Option<i64>, Option<f64>) {
        match value {
            CacheNumericValue::Integer(value) => ("integer", Some(Self::u64_to_i64(*value)), None),
            CacheNumericValue::Float(value) => ("float", None, Some(*value)),
        }
    }

    pub(super) fn cache_numeric_from_columns(
        kind: &str,
        int_value: Option<i64>,
        float_value: Option<f64>,
    ) -> CacheNumericValue {
        if kind == "float" {
            CacheNumericValue::Float(
                float_value.unwrap_or_else(|| int_value.map_or(0.0, |value| value as f64)),
            )
        } else {
            CacheNumericValue::Integer(int_value.map(Self::i64_to_u64).unwrap_or_default())
        }
    }

    pub(super) fn protocol_build_columns(
        value: &ProtocolBuildValue,
    ) -> (&'static str, Option<i64>, Option<String>) {
        match value {
            ProtocolBuildValue::Int(value) => ("integer", Some(i64::from(*value)), None),
            ProtocolBuildValue::Str(value) => ("text", None, Some(value.clone())),
        }
    }

    pub(super) fn protocol_build_from_columns(
        kind: &str,
        int_value: Option<i64>,
        text_value: Option<String>,
    ) -> ProtocolBuildValue {
        if kind == "text" {
            ProtocolBuildValue::Str(text_value.unwrap_or_default())
        } else {
            ProtocolBuildValue::Int(int_value.map(Self::i64_to_u32).unwrap_or_default())
        }
    }
}

#[derive(Debug)]
pub(super) struct ReplayCacheFileName {
    value: String,
}

impl ReplayCacheFileName {
    pub(super) fn from_replay_file(file: &str) -> Self {
        let value = file
            .rsplit(['/', '\\'])
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(file)
            .to_string();
        Self { value }
    }

    pub(super) fn into_string(self) -> String {
        self.value
    }
}

pub struct ReplayCacheDatabase {
    pub(super) cache_path: PathBuf,
    legacy_cache_path: PathBuf,
    pub(super) db_path: PathBuf,
    pub(super) connection: Connection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqliteReplayCacheEntrySink {
    cache_path: PathBuf,
}

impl SqliteReplayCacheEntrySink {
    pub fn new(cache_path: impl Into<PathBuf>) -> Self {
        Self {
            cache_path: cache_path.into(),
        }
    }

    pub fn cache_path(&self) -> &Path {
        &self.cache_path
    }
}

impl CacheEntrySink for SqliteReplayCacheEntrySink {
    fn write_entries(&self, entries: &[CacheReplayEntry]) -> Result<usize, CacheEntrySinkError> {
        ReplayCacheDatabase::open_for_cache_path(&self.cache_path)
            .and_then(|mut database| database.upsert_entries_preserving_detailed(entries))
            .map_err(|error| CacheEntrySinkError::new(error.to_string()))
    }
}

impl ReplayCacheDatabase {
    pub(super) fn retry_sqlite_lock<T>(
        mut operation: impl FnMut() -> Result<T, ReplayCacheDbError>,
    ) -> Result<T, ReplayCacheDbError> {
        let started_at = Instant::now();
        loop {
            match operation() {
                Ok(value) => return Ok(value),
                Err(error)
                    if error.is_sqlite_lock()
                        && started_at.elapsed() < SQLITE_LOCK_RETRY_WINDOW =>
                {
                    thread::sleep(SQLITE_LOCK_RETRY_DELAY);
                }
                Err(error) => return Err(error),
            }
        }
    }

    pub fn db_path_for_cache_path(cache_path: &Path) -> PathBuf {
        if cache_path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("sqlite3"))
        {
            cache_path.to_path_buf()
        } else {
            cache_path.with_extension("sqlite3")
        }
    }

    pub fn legacy_json_path_for_cache_path(cache_path: &Path) -> PathBuf {
        if cache_path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("sqlite3"))
        {
            cache_path.with_extension("json")
        } else {
            cache_path.to_path_buf()
        }
    }

    pub fn legacy_temp_jsonl_path_for_cache_path(cache_path: &Path) -> PathBuf {
        Self::legacy_json_path_for_cache_path(cache_path).with_extension("temp.jsonl")
    }

    pub fn db_related_paths_for_cache_path(cache_path: &Path) -> Vec<PathBuf> {
        let db_path = Self::db_path_for_cache_path(cache_path);
        vec![
            db_path.clone(),
            Self::path_with_suffix(&db_path, "-wal"),
            Self::path_with_suffix(&db_path, "-shm"),
            Self::path_with_suffix(&db_path, "-journal"),
        ]
    }

    fn path_with_suffix(path: &Path, suffix: &str) -> PathBuf {
        let mut value = path.as_os_str().to_os_string();
        value.push(suffix);
        PathBuf::from(value)
    }

    pub(super) fn sqlite_contains_pattern(value: &str) -> String {
        let mut pattern = String::with_capacity(value.len() + 2);
        pattern.push('%');
        for ch in value.trim().to_ascii_lowercase().chars() {
            match ch {
                '%' | '_' | '\\' => {
                    pattern.push('\\');
                    pattern.push(ch);
                }
                _ => pattern.push(ch),
            }
        }
        pattern.push('%');
        pattern
    }

    pub(super) fn usize_to_i64(value: usize) -> i64 {
        i64::try_from(value).unwrap_or(i64::MAX)
    }

    pub fn open_for_cache_path(cache_path: &Path) -> Result<Self, ReplayCacheDbError> {
        Self::retry_sqlite_lock(|| Self::open_for_cache_path_once(cache_path))
    }

    fn open_for_cache_path_once(cache_path: &Path) -> Result<Self, ReplayCacheDbError> {
        let db_path = Self::db_path_for_cache_path(cache_path);
        let legacy_cache_path = Self::legacy_json_path_for_cache_path(cache_path);
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| ReplayCacheDbError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let db_existed = db_path.exists();
        let mut connection =
            Connection::open(&db_path).map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.clone(),
                source,
            })?;
        connection
            .busy_timeout(SQLITE_BUSY_TIMEOUT)
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.clone(),
                source,
            })?;
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.clone(),
                source,
            })?;
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.clone(),
                source,
            })?;

        Self::initialize_schema(&mut connection, &db_path)?;
        let mut database = Self {
            cache_path: cache_path.to_path_buf(),
            legacy_cache_path,
            db_path,
            connection,
        };
        if let Err(error) = database.import_legacy_cache_if_needed(db_existed) {
            crate::sco_log!("[SCO/cache-db] legacy cache import skipped: {error}");
        }
        Ok(database)
    }

    fn initialize_schema(
        connection: &mut Connection,
        db_path: &Path,
    ) -> Result<(), ReplayCacheDbError> {
        let schema_version = connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, i32>(0))
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.to_path_buf(),
                source,
            })?;

        if schema_version > CURRENT_SCHEMA_VERSION {
            return Err(ReplayCacheDbError::UnsupportedSchema {
                path: db_path.to_path_buf(),
                version: schema_version,
                supported: CURRENT_SCHEMA_VERSION,
            });
        }

        if schema_version == CURRENT_SCHEMA_VERSION {
            return Ok(());
        }

        Self::create_current_schema(connection, db_path)?;
        Self::create_current_indexes(connection, db_path)?;
        Ok(())
    }

    fn create_current_schema(
        connection: &mut Connection,
        db_path: &Path,
    ) -> Result<(), ReplayCacheDbError> {
        connection
            .execute_batch(
                "
                CREATE TABLE IF NOT EXISTS replay_cache_entries (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    hash TEXT UNIQUE NOT NULL,
                    file TEXT NOT NULL,
                    file_name TEXT NOT NULL,
                    date_text TEXT NOT NULL,
                    date_seconds INTEGER NOT NULL,
                    detailed_analysis INTEGER NOT NULL,
                    result TEXT NOT NULL,
                    map_name TEXT NOT NULL,
                    difficulty_p1 TEXT NOT NULL,
                    difficulty_p2 TEXT NOT NULL,
                    ext_difficulty TEXT NOT NULL,
                    brutal_plus INTEGER NOT NULL,
                    extension INTEGER NOT NULL,
                    weekly INTEGER NOT NULL,
                    region TEXT NOT NULL,
                    length_ingame_seconds INTEGER NOT NULL,
                    length_realtime_kind TEXT NOT NULL CHECK(length_realtime_kind IN ('integer', 'float')),
                    length_realtime_int INTEGER,
                    length_realtime_float REAL,
                    form_length_realtime TEXT NOT NULL,
                    replay_build INTEGER NOT NULL,
                    protocol_build_kind TEXT NOT NULL CHECK(protocol_build_kind IN ('integer', 'text')),
                    protocol_build_int INTEGER,
                    protocol_build_text TEXT,
                    comp TEXT,
                    enemy_race TEXT,
                    has_amon_units INTEGER NOT NULL,
                    has_bonus INTEGER NOT NULL,
                    has_player_stats INTEGER NOT NULL,
                    mutator_values TEXT NOT NULL CHECK(json_valid(mutator_values)),
                    bonus_values TEXT NOT NULL CHECK(json_valid(bonus_values)),
                    updated_at_seconds INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS replay_player_infos (
                    handle TEXT PRIMARY KEY NOT NULL,
                    wins INTEGER NOT NULL,
                    losses INTEGER NOT NULL,
                    average_apm REAL NOT NULL,
                    latest_commander TEXT NOT NULL,
                    commander_frequency REAL NOT NULL,
                    kill_ratio REAL NOT NULL,
                    latest_played_time INTEGER NOT NULL,
                    updated_at_seconds INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS replay_cache_players (
                    replay_id INTEGER NOT NULL REFERENCES replay_cache_entries(id) ON DELETE CASCADE,
                    pid INTEGER NOT NULL CHECK(pid > 0),
                    player_name TEXT NOT NULL,
                    apm INTEGER,
                    commander TEXT,
                    commander_level INTEGER,
                    commander_mastery_level INTEGER,
                    player_handle TEXT NOT NULL REFERENCES replay_player_infos(handle) ON UPDATE CASCADE,
                    kills INTEGER,
                    observer INTEGER,
                    prestige INTEGER,
                    prestige_name TEXT,
                    race TEXT,
                    result TEXT,
                    has_masteries INTEGER NOT NULL,
                    has_icons INTEGER NOT NULL,
                    has_units INTEGER NOT NULL,
                    mastery_values TEXT NOT NULL CHECK(json_valid(mastery_values)),
                    PRIMARY KEY (replay_id, pid)
                );

                CREATE TABLE IF NOT EXISTS replay_cache_weeklies (
                    replay_id INTEGER PRIMARY KEY REFERENCES replay_cache_entries(id) ON DELETE CASCADE,
                    result TEXT NOT NULL,
                    map_name TEXT NOT NULL,
                    difficulty TEXT NOT NULL,
                    brutal_plus INTEGER NOT NULL,
                    mutator_values TEXT NOT NULL CHECK(json_valid(mutator_values))
                );

                CREATE TABLE IF NOT EXISTS replay_cache_player_units (
                    replay_id INTEGER NOT NULL REFERENCES replay_cache_entries(id) ON DELETE CASCADE,
                    pid INTEGER NOT NULL CHECK(pid > 0),
                    unit_name TEXT NOT NULL,
                    created_kind TEXT NOT NULL CHECK(created_kind IN ('count', 'hidden')),
                    created_count INTEGER,
                    lost_kind TEXT NOT NULL CHECK(lost_kind IN ('count', 'hidden')),
                    lost_count INTEGER,
                    kills INTEGER NOT NULL,
                    fraction REAL NOT NULL,
                    PRIMARY KEY (replay_id, pid, unit_name)
                );

                CREATE TABLE IF NOT EXISTS replay_cache_player_icons (
                    replay_id INTEGER NOT NULL REFERENCES replay_cache_entries(id) ON DELETE CASCADE,
                    pid INTEGER NOT NULL CHECK(pid > 0),
                    icon_name TEXT NOT NULL,
                    icon_kind TEXT NOT NULL CHECK(icon_kind IN ('count', 'order')),
                    count_value INTEGER,
                    PRIMARY KEY (replay_id, pid, icon_name)
                );

                CREATE TABLE IF NOT EXISTS replay_cache_player_icon_orders (
                    replay_id INTEGER NOT NULL REFERENCES replay_cache_entries(id) ON DELETE CASCADE,
                    pid INTEGER NOT NULL CHECK(pid > 0),
                    icon_name TEXT NOT NULL,
                    order_values TEXT NOT NULL CHECK(json_valid(order_values)),
                    PRIMARY KEY (replay_id, pid, icon_name)
                );

                CREATE TABLE IF NOT EXISTS replay_cache_messages (
                    replay_id INTEGER NOT NULL REFERENCES replay_cache_entries(id) ON DELETE CASCADE,
                    message_index INTEGER NOT NULL,
                    player INTEGER NOT NULL,
                    time REAL NOT NULL,
                    text TEXT NOT NULL,
                    PRIMARY KEY (replay_id, message_index)
                );

                CREATE TABLE IF NOT EXISTS replay_cache_amon_units (
                    replay_id INTEGER NOT NULL REFERENCES replay_cache_entries(id) ON DELETE CASCADE,
                    unit_name TEXT NOT NULL,
                    created_kind TEXT NOT NULL CHECK(created_kind IN ('count', 'hidden')),
                    created_count INTEGER,
                    created_hidden TEXT,
                    lost_kind TEXT NOT NULL CHECK(lost_kind IN ('count', 'hidden')),
                    lost_count INTEGER,
                    lost_hidden TEXT,
                    kills INTEGER NOT NULL,
                    fraction REAL NOT NULL,
                    PRIMARY KEY (replay_id, unit_name)
                );

                CREATE TABLE IF NOT EXISTS replay_cache_player_stat_series (
                    replay_id INTEGER NOT NULL REFERENCES replay_cache_entries(id) ON DELETE CASCADE,
                    pid INTEGER NOT NULL CHECK(pid > 0),
                    player_handle TEXT NOT NULL REFERENCES replay_player_infos(handle) ON UPDATE CASCADE,
                    supply_values TEXT NOT NULL CHECK(json_valid(supply_values)),
                    mining_values TEXT NOT NULL CHECK(json_valid(mining_values)),
                    army_values TEXT NOT NULL CHECK(json_valid(army_values)),
                    killed_values TEXT NOT NULL CHECK(json_valid(killed_values)),
                    PRIMARY KEY (replay_id, pid)
                );

                PRAGMA user_version = 1;
                ",
            )
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.to_path_buf(),
                source,
            })
    }

    fn create_current_indexes(
        connection: &mut Connection,
        db_path: &Path,
    ) -> Result<(), ReplayCacheDbError> {
        connection
            .execute_batch(
                "
                CREATE INDEX IF NOT EXISTS idx_replay_cache_entries_date
                    ON replay_cache_entries(date_seconds DESC, date_text DESC, file DESC, hash DESC);
                CREATE INDEX IF NOT EXISTS idx_replay_cache_entries_file
                    ON replay_cache_entries(file);
                CREATE INDEX IF NOT EXISTS idx_replay_cache_entries_file_name
                    ON replay_cache_entries(file_name, date_seconds DESC);
                CREATE INDEX IF NOT EXISTS idx_replay_cache_entries_detailed
                    ON replay_cache_entries(detailed_analysis, date_seconds DESC);
                CREATE INDEX IF NOT EXISTS idx_replay_cache_entries_stats_filter
                    ON replay_cache_entries(detailed_analysis, extension, date_seconds DESC, brutal_plus, result);
                CREATE INDEX IF NOT EXISTS idx_replay_cache_entries_games_tab
                    ON replay_cache_entries(date_seconds DESC, result, difficulty_p1, difficulty_p2, map_name);
                CREATE INDEX IF NOT EXISTS idx_replay_cache_players_handle
                    ON replay_cache_players(player_handle, replay_id);
                CREATE INDEX IF NOT EXISTS idx_replay_cache_players_name
                    ON replay_cache_players(player_handle, player_name, replay_id);
                CREATE INDEX IF NOT EXISTS idx_replay_cache_players_stats_name
                    ON replay_cache_players(player_name COLLATE NOCASE, replay_id);
                CREATE INDEX IF NOT EXISTS idx_replay_cache_players_commander
                    ON replay_cache_players(commander, replay_id);
                CREATE INDEX IF NOT EXISTS idx_replay_player_infos_last_played
                    ON replay_player_infos(latest_played_time DESC, handle ASC);
                CREATE INDEX IF NOT EXISTS idx_replay_player_infos_wins
                    ON replay_player_infos(wins DESC, handle ASC);
                CREATE INDEX IF NOT EXISTS idx_replay_player_infos_losses
                    ON replay_player_infos(losses DESC, handle ASC);
                CREATE INDEX IF NOT EXISTS idx_replay_player_infos_apm
                    ON replay_player_infos(average_apm DESC, handle ASC);
                CREATE INDEX IF NOT EXISTS idx_replay_player_infos_commander
                    ON replay_player_infos(latest_commander, latest_played_time DESC);
                CREATE INDEX IF NOT EXISTS idx_replay_player_infos_kill_ratio
                    ON replay_player_infos(kill_ratio DESC, handle ASC);
                CREATE INDEX IF NOT EXISTS idx_replay_cache_weeklies_mutation
                    ON replay_cache_weeklies(map_name, brutal_plus);
                CREATE INDEX IF NOT EXISTS idx_replay_cache_player_units_unit
                    ON replay_cache_player_units(unit_name, replay_id);
                CREATE INDEX IF NOT EXISTS idx_replay_cache_amon_units_unit
                    ON replay_cache_amon_units(unit_name, replay_id);
                ",
            )
            .map_err(|source| ReplayCacheDbError::Sqlite {
                path: db_path.to_path_buf(),
                source,
            })?;
        Ok(())
    }

    fn import_legacy_cache_if_needed(
        &mut self,
        db_existed: bool,
    ) -> Result<(), ReplayCacheDbError> {
        let import_json = self.should_import_legacy_json(db_existed);
        if import_json {
            self.import_legacy_cache_file()?;
        }

        let temp_path = Self::legacy_temp_jsonl_path_for_cache_path(&self.cache_path);
        if temp_path.exists() {
            self.import_temp_cache_file(&temp_path)?;
        }

        Ok(())
    }

    fn should_import_legacy_json(&self, db_existed: bool) -> bool {
        if !self.legacy_cache_path.exists() {
            return false;
        }
        if !db_existed {
            return true;
        }

        let Ok(json_modified) = self
            .legacy_cache_path
            .metadata()
            .and_then(|meta| meta.modified())
        else {
            return false;
        };
        let Ok(db_modified) = self.db_path.metadata().and_then(|meta| meta.modified()) else {
            return true;
        };
        json_modified > db_modified
    }

    pub fn import_legacy_cache_file(&mut self) -> Result<usize, ReplayCacheDbError> {
        let mut entries = self.read_legacy_cache_entries()?;
        Self::normalize_legacy_cache_dates_to_utc(&mut entries);
        let changed = self.upsert_entries_preserving_detailed(&entries)?;
        Self::remove_imported_legacy_file(&self.legacy_cache_path)?;
        Ok(changed)
    }

    fn import_temp_cache_file(&mut self, temp_path: &Path) -> Result<usize, ReplayCacheDbError> {
        let mut entries = Self::read_temp_cache_entries(temp_path)?;
        Self::normalize_legacy_cache_dates_to_utc(&mut entries);
        let changed = self.upsert_entries_preserving_detailed(&entries)?;
        Self::remove_imported_legacy_file(temp_path)?;
        Ok(changed)
    }

    fn remove_imported_legacy_file(path: &Path) -> Result<(), ReplayCacheDbError> {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(ReplayCacheDbError::Io {
                path: path.to_path_buf(),
                source,
            }),
        }
    }

    fn read_legacy_cache_entries(&self) -> Result<Vec<CacheReplayEntry>, ReplayCacheDbError> {
        let payload =
            std::fs::read(&self.legacy_cache_path).map_err(|source| ReplayCacheDbError::Io {
                path: self.legacy_cache_path.clone(),
                source,
            })?;
        serde_json::from_slice::<Vec<CacheReplayEntry>>(&payload).map_err(|source| {
            ReplayCacheDbError::Json {
                path: self.legacy_cache_path.clone(),
                source,
            }
        })
    }

    fn normalize_legacy_cache_dates_to_utc(entries: &mut [CacheReplayEntry]) {
        for entry in entries {
            if let Some(date_text) = Self::legacy_local_date_to_utc_text(&entry.date) {
                entry.date = date_text;
            }
        }
    }

    fn legacy_local_date_to_utc_text(value: &str) -> Option<String> {
        let parts = value
            .split(|ch: char| !ch.is_ascii_digit())
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        if parts.len() < 3 {
            return None;
        }

        let year = parts.first()?.parse::<i32>().ok()?;
        let month = parts.get(1)?.parse::<u32>().ok()?;
        let day = parts.get(2)?.parse::<u32>().ok()?;
        let hour = parts
            .get(3)
            .and_then(|part| part.parse::<u32>().ok())
            .unwrap_or(0);
        let minute = parts
            .get(4)
            .and_then(|part| part.parse::<u32>().ok())
            .unwrap_or(0);
        let second = parts
            .get(5)
            .and_then(|part| part.parse::<u32>().ok())
            .unwrap_or(0);

        let local_datetime = match Local.with_ymd_and_hms(year, month, day, hour, minute, second) {
            LocalResult::Single(value) => value,
            LocalResult::Ambiguous(earliest, _) => earliest,
            LocalResult::None => return None,
        };
        Some(
            local_datetime
                .with_timezone(&Utc)
                .format("%Y:%m:%d:%H:%M:%S")
                .to_string(),
        )
    }

    fn read_temp_cache_entries(
        temp_path: &Path,
    ) -> Result<Vec<CacheReplayEntry>, ReplayCacheDbError> {
        let content =
            std::fs::read_to_string(temp_path).map_err(|source| ReplayCacheDbError::Io {
                path: temp_path.to_path_buf(),
                source,
            })?;
        let mut entries = Vec::new();
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            match serde_json::from_str::<CacheReplayEntry>(trimmed) {
                Ok(entry) if !entry.hash.is_empty() => entries.push(entry),
                Ok(_) => {}
                Err(source) => {
                    crate::sco_log!(
                        "[SCO/cache-db] ignored malformed temp cache row '{}': {source}",
                        temp_path.display()
                    );
                }
            }
        }
        Ok(entries)
    }

    pub(super) fn count_value_columns(value: &CacheCountValue) -> (&'static str, Option<i64>) {
        match value {
            CacheCountValue::Count(value) => ("count", Some(*value)),
            CacheCountValue::Hidden(_) => ("hidden", None),
        }
    }

    pub(super) fn count_value_columns_with_hidden(
        value: &CacheCountValue,
    ) -> (&'static str, Option<i64>, Option<String>) {
        match value {
            CacheCountValue::Count(value) => ("count", Some(*value), None),
            CacheCountValue::Hidden(value) => ("hidden", None, Some(value.clone())),
        }
    }

    pub(super) fn count_value_from_kind_and_count(
        kind: String,
        count: Option<i64>,
    ) -> CacheCountValue {
        if kind == "hidden" {
            CacheCountValue::Hidden("-".to_string())
        } else {
            CacheCountValue::Count(count.unwrap_or_default())
        }
    }

    pub(super) fn count_value_from_columns(
        kind: String,
        count: Option<i64>,
        hidden: Option<String>,
    ) -> CacheCountValue {
        if kind == "hidden" {
            CacheCountValue::Hidden(hidden.unwrap_or_default())
        } else {
            CacheCountValue::Count(count.unwrap_or_default())
        }
    }

    pub(super) fn sqlite_error(&self, source: rusqlite::Error) -> ReplayCacheDbError {
        ReplayCacheDbError::Sqlite {
            path: self.db_path.clone(),
            source,
        }
    }

    pub(super) fn now_seconds() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0)
    }
}
