use super::*;

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

    pub fn scope(&self) -> ReplayCacheReadScope {
        self.scope
    }

    pub fn limit(&self) -> usize {
        self.limit
    }

    pub fn include_mutations(&self) -> bool {
        self.include_mutations
    }

    pub fn include_normal_games(&self) -> bool {
        self.include_normal_games
    }

    pub fn include_wins(&self) -> bool {
        self.include_wins
    }

    pub fn include_losses(&self) -> bool {
        self.include_losses
    }

    pub fn min_length_seconds(&self) -> u64 {
        self.min_length_seconds
    }

    pub fn max_length_seconds(&self) -> u64 {
        self.max_length_seconds
    }

    pub fn min_date_seconds(&self) -> Option<u64> {
        self.min_date_seconds
    }

    pub fn max_date_seconds(&self) -> Option<u64> {
        self.max_date_seconds
    }

    pub fn player_filter(&self) -> &str {
        &self.player_filter
    }

    pub fn difficulty_exclusions(&self) -> &[ReplayCacheStatsDifficultyExclusion] {
        &self.difficulty_exclusions
    }

    pub fn region_exclusions(&self) -> &[String] {
        &self.region_exclusions
    }

    pub fn current_replay_files(&self) -> &[String] {
        &self.current_replay_files
    }

    pub fn restrict_to_current_replay_files(&self) -> bool {
        self.restrict_to_current_replay_files
    }

    pub fn include_sub_15(&self) -> bool {
        self.include_sub_15
    }

    pub fn include_over_15(&self) -> bool {
        self.include_over_15
    }

    pub fn include_ally_sub_15(&self) -> bool {
        self.include_ally_sub_15
    }

    pub fn include_ally_over_15(&self) -> bool {
        self.include_ally_over_15
    }

    pub fn include_main_normal_mastery(&self) -> bool {
        self.include_main_normal_mastery
    }

    pub fn include_main_abnormal_mastery(&self) -> bool {
        self.include_main_abnormal_mastery
    }

    pub fn include_ally_normal_mastery(&self) -> bool {
        self.include_ally_normal_mastery
    }

    pub fn include_ally_abnormal_mastery(&self) -> bool {
        self.include_ally_abnormal_mastery
    }

    pub fn include_both_main(&self) -> bool {
        self.include_both_main
    }

    pub fn main_handle_keys(&self) -> &[String] {
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

    pub fn sql_keyword(self) -> &'static str {
        match self {
            Self::Asc => "ASC",
            Self::Desc => "DESC",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReplayCachePage {
    rows_per_page: usize,
    offset: usize,
}

impl ReplayCachePage {
    pub fn new(page: usize, rows_per_page: usize) -> Self {
        let page = page.max(1);
        let rows_per_page = rows_per_page.max(1);
        Self {
            rows_per_page,
            offset: page.saturating_sub(1).saturating_mul(rows_per_page),
        }
    }

    pub fn from_offset(offset: usize, rows_per_page: usize) -> Self {
        let rows_per_page = rows_per_page.max(1);
        Self {
            rows_per_page,
            offset,
        }
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    pub fn limit(&self) -> usize {
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
    pub fn new(
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

    pub fn brutal_plus_level(self) -> Option<i64> {
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

    pub fn regular_label(self) -> Option<&'static str> {
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

    pub fn page(&self) -> ReplayCachePage {
        self.page
    }

    pub fn with_page(&self, page: ReplayCachePage) -> Self {
        Self {
            page,
            search: self.search.clone(),
            sort_key: self.sort_key,
            sort_direction: self.sort_direction,
            difficulty_filters: self.difficulty_filters.clone(),
            include_normal_games: self.include_normal_games,
            include_mutation_games: self.include_mutation_games,
        }
    }

    pub fn search(&self) -> &str {
        &self.search
    }

    pub fn sort_key(&self) -> ReplayCacheGameSortKey {
        self.sort_key
    }

    pub fn sort_direction(&self) -> ReplayCacheSortDirection {
        self.sort_direction
    }

    pub fn difficulty_filters(&self) -> &[ReplayCacheDifficultyFilter] {
        &self.difficulty_filters
    }

    pub fn include_normal_games(&self) -> bool {
        self.include_normal_games
    }

    pub fn include_mutation_games(&self) -> bool {
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

    pub fn handle(&self) -> &str {
        &self.handle
    }

    pub fn note(&self) -> &str {
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

    pub fn page(&self) -> ReplayCachePage {
        self.page
    }

    pub fn search(&self) -> &str {
        &self.search
    }

    pub fn sort_key(&self) -> ReplayCachePlayerSortKey {
        self.sort_key
    }

    pub fn sort_direction(&self) -> ReplayCacheSortDirection {
        self.sort_direction
    }

    pub fn notes(&self) -> &[ReplayCachePlayerNote] {
        &self.notes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayCacheTable {
    Weekly,
    Players,
    PlayerUnits,
    StatisticsPlayers,
    StatisticsPlayerUnits,
    PlayerIcons,
    PlayerIconOrders,
    Messages,
    AmonUnits,
    PlayerStatSeries,
}

pub const REPLAY_CACHE_CHILD_TABLES: [ReplayCacheTable; 10] = [
    ReplayCacheTable::Weekly,
    ReplayCacheTable::StatisticsPlayerUnits,
    ReplayCacheTable::StatisticsPlayers,
    ReplayCacheTable::PlayerUnits,
    ReplayCacheTable::PlayerIconOrders,
    ReplayCacheTable::PlayerIcons,
    ReplayCacheTable::Messages,
    ReplayCacheTable::AmonUnits,
    ReplayCacheTable::PlayerStatSeries,
    ReplayCacheTable::Players,
];

impl ReplayCacheTable {
    pub fn delete_by_replay_id_sql(self) -> &'static str {
        match self {
            Self::Players => "DELETE FROM replay_cache_players WHERE replay_id = ?1",
            Self::Weekly => "DELETE FROM replay_cache_weeklies WHERE replay_id = ?1",
            Self::PlayerUnits => "DELETE FROM replay_cache_player_units WHERE replay_id = ?1",
            Self::StatisticsPlayers => {
                "DELETE FROM replay_cache_stats_players WHERE replay_id = ?1"
            }
            Self::StatisticsPlayerUnits => {
                "DELETE FROM replay_cache_stats_player_units WHERE replay_id = ?1"
            }
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

pub const REPLAY_CACHE_ENTRY_RECORD_COLUMNS: &str = "
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
pub enum ReplayCacheEntryRecordQuery {
    All,
    AllLimited,
    DetailedOnly,
    DetailedOnlyLimited,
}

impl ReplayCacheEntryRecordQuery {
    pub fn from_entry_query(query: ReplayCacheEntryQuery) -> Self {
        match (query.scope(), query.limit()) {
            (ReplayCacheReadScope::All, 0) => Self::All,
            (ReplayCacheReadScope::All, _) => Self::AllLimited,
            (ReplayCacheReadScope::DetailedOnly, 0) => Self::DetailedOnly,
            (ReplayCacheReadScope::DetailedOnly, _) => Self::DetailedOnlyLimited,
        }
    }

    pub fn limit(self, query: ReplayCacheEntryQuery) -> Option<i64> {
        match self {
            Self::All | Self::DetailedOnly => None,
            Self::AllLimited | Self::DetailedOnlyLimited => {
                Some(i64::try_from(query.limit()).unwrap_or(i64::MAX))
            }
        }
    }

    pub fn sql(self) -> String {
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
