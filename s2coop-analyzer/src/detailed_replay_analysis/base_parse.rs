use super::timing::{ReplayBaseParseTiming, ReplayEntryParseTiming};
use super::{
    DetailedReplayAnalysisError, DetailedReplayAnalyzer, ProtocolBuildValue,
    ReplayAnalysisResources, ReplayAnalysisSets, ReplayBaseParse, ReplayBaseParseError,
    ReplayBaseParseFilters, ReplayBaseParseOptions, ReplayBuildInfo, ReplayCacheContext,
    ReplayDetailedEventCollector, ReplayDetailedParseContext, ReplayEventKind, ReplayFileDigest,
    ReplayMutatorIdentificationInput, ReplayMutatorParseContext, ReplayNumericValue,
    ReplayParsedContext, ReplayParsedInputBundle, TimedReplayEntryParse,
};
use crate::cache_overall_stats_generator::{CacheOverallStatsFile, CacheReplayEntry};
use crate::dictionary_data::{CacheGenerationData, Sc2DictionaryData};
use crate::stats_counter_core::StatsCounterDictionaries;
use crate::tauri_replay_analysis_impl::{
    ParsedReplayInput, ParsedReplayMessage, ParsedReplayPlayer,
};
use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use s2protocol_port::{
    ProtocolStore, ProtocolStoreBuilder, ReplayDetails, ReplayEvent, ReplayInitData,
    ReplayParseMode, ReplayParseOptions, ReplayParser,
};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{Instant, UNIX_EPOCH};

impl ReplayAnalysisResources {
    pub fn from_dictionary_data(
        dictionary_data: Arc<Sc2DictionaryData>,
    ) -> Result<Self, DetailedReplayAnalysisError> {
        let hidden_created_lost = dictionary_data
            .replay_analysis_data
            .dont_show_created_lost
            .iter()
            .cloned()
            .collect::<HashSet<String>>();
        let protocol_store = ProtocolStoreBuilder::build().map_err(|error| {
            DetailedReplayAnalysisError::ProtocolStore(format!(
                "failed to build protocol store: {error}"
            ))
        })?;
        let analysis_sets = ReplayAnalysisSets::new(dictionary_data.as_ref());
        let stats_counter_dictionaries =
            Arc::new(DetailedReplayAnalyzer::build_stats_counter_dictionaries(
                &dictionary_data.cache_generation_data(),
            ));

        Ok(Self {
            dictionary_data,
            hidden_created_lost,
            analysis_sets,
            stats_counter_dictionaries,
            protocol_store,
        })
    }

    pub fn dictionary_data(&self) -> &Sc2DictionaryData {
        self.dictionary_data.as_ref()
    }

    pub fn cache_generation_data(&self) -> CacheGenerationData<'_> {
        self.dictionary_data.cache_generation_data()
    }

    pub fn hidden_created_lost(&self) -> &HashSet<String> {
        &self.hidden_created_lost
    }

    pub(super) fn analysis_sets(&self) -> &ReplayAnalysisSets {
        &self.analysis_sets
    }

    pub(super) fn stats_counter_dictionaries(&self) -> Arc<StatsCounterDictionaries> {
        Arc::clone(&self.stats_counter_dictionaries)
    }

    pub fn protocol_store(&self) -> &ProtocolStore {
        &self.protocol_store
    }

    fn parse_replay_base(
        &self,
        replay_path: &Path,
        options: ReplayBaseParseOptions,
    ) -> Result<Option<ReplayBaseParse>, ReplayBaseParseError> {
        self.parse_replay_base_timed(replay_path, options).0
    }

    fn parse_replay_base_timed(
        &self,
        replay_path: &Path,
        options: ReplayBaseParseOptions,
    ) -> (
        Result<Option<ReplayBaseParse>, ReplayBaseParseError>,
        ReplayBaseParseTiming,
    ) {
        let inputs = self.cache_generation_data();
        DetailedReplayAnalyzer::parse_replay_base_timed(
            replay_path,
            &inputs,
            self.protocol_store(),
            options,
        )
    }
}

impl DetailedReplayAnalyzer {
    pub fn is_games_tab_custom_replay_path(path: &Path) -> bool {
        Self::is_mm_replay_path(path)
            || path
                .file_name()
                .and_then(|file_name| file_name.to_str())
                .is_some_and(Self::replay_file_name_contains_coop)
    }

    pub fn is_mm_replay_path(path: &Path) -> bool {
        Self::is_mm_replay_file(&path.to_string_lossy())
    }

    pub fn is_mm_replay_file(file: &str) -> bool {
        file.contains("[MM]")
    }

    fn replay_file_name_contains_coop(file_name: &str) -> bool {
        let normalized_file_name = file_name
            .chars()
            .filter(|character| character.is_alphanumeric())
            .flat_map(|character| character.to_lowercase())
            .collect::<String>();
        normalized_file_name.contains("coop")
    }

    fn replay_game_speed_code(details: &ReplayDetails, init_data: &ReplayInitData) -> i64 {
        if matches!(details.m_gameSpeed, 0..=4) {
            details.m_gameSpeed
        } else if matches!(
            init_data.m_syncLobbyState.m_gameDescription.m_gameSpeed,
            0..=4
        ) {
            init_data.m_syncLobbyState.m_gameDescription.m_gameSpeed
        } else {
            4
        }
    }

    fn game_speed_multiplier(game_speed: i64) -> f64 {
        match game_speed {
            0 => 0.6,
            1 => 0.8,
            2 => 1.0,
            3 => 1.2,
            4 => 1.4,
            _ => 1.4,
        }
    }

    pub(super) fn realtime_length_from_replay(
        accurate_length: f64,
        details: &ReplayDetails,
        init_data: &ReplayInitData,
    ) -> f64 {
        DetailedReplayAnalyzer::realtime_length_from_game_speed(
            accurate_length,
            DetailedReplayAnalyzer::replay_game_speed_code(details, init_data),
        )
    }

    fn realtime_length_from_game_speed(accurate_length: f64, game_speed_code: i64) -> f64 {
        if !accurate_length.is_finite() || accurate_length <= 0.0 {
            return 0.0;
        }

        let multiplier = DetailedReplayAnalyzer::game_speed_multiplier(game_speed_code);

        accurate_length / multiplier
    }

    fn parse_replay_base_timed(
        replay_path: &Path,
        inputs: &CacheGenerationData<'_>,
        protocol_store: &ProtocolStore,
        options: ReplayBaseParseOptions,
    ) -> (
        Result<Option<ReplayBaseParse>, ReplayBaseParseError>,
        ReplayBaseParseTiming,
    ) {
        let total_start = Instant::now();
        let mut timing = ReplayBaseParseTiming::default();
        let result = (|| -> Result<Option<ReplayBaseParse>, ReplayBaseParseError> {
            let early_filter_start = Instant::now();
            let replay_file = replay_path.to_string_lossy();
            let is_mm_replay = Self::is_mm_replay_file(&replay_file);
            if options.filters.only_blizzard && is_mm_replay {
                timing.early_filter = early_filter_start.elapsed();
                return Ok(None);
            }
            timing.early_filter = early_filter_start.elapsed();

            let decode_replay_start = Instant::now();
            let (mut parsed, detailed_event_collection) = if options.include_events {
                let mut event_collector = ReplayDetailedEventCollector::new();
                let mut parsed =
                    ReplayParser::parse_file_with_store_ordered_events_filtered_retained_options(
                        replay_path,
                        protocol_store,
                        ReplayEventKind::needed_for_detailed_analysis_name,
                        |event| event_collector.observe_and_retain_for_report(event),
                        ReplayParseOptions::new().with_decode_attributes(false),
                    )
                    .map_err(|error| ReplayBaseParseError::ReplayParse {
                        path: replay_path.display().to_string(),
                        message: error.to_string(),
                    })?;
                timing.decode_replay_detail.add(parsed.timing());
                let ordered_events_decoded_count = parsed.ordered_events_decoded_count();
                let events = parsed.take_events();
                let collection = event_collector.finish(events, ordered_events_decoded_count);
                timing.events_decoded_len = collection.decoded_event_count;
                timing.events_decoded_capacity = collection
                    .decoded_event_count
                    .max(collection.events.capacity());
                timing.events_retained_len = collection.events.len();
                timing.events_retained_capacity = collection.events.capacity();
                (parsed.take_replay(), Some(collection))
            } else {
                let parsed = ReplayParser::parse_file_with_store_timed(
                    replay_path,
                    protocol_store,
                    ReplayParseMode::Simple,
                )
                .map_err(|error| ReplayBaseParseError::ReplayParse {
                    path: replay_path.display().to_string(),
                    message: error.to_string(),
                })?;
                timing.decode_replay_detail.add(parsed.timing());
                (parsed.take_replay(), None)
            };
            timing.decode_replay = decode_replay_start.elapsed();

            let extract_fields_start = Instant::now();
            let base_build = parsed.base_build();
            let details = parsed.take_details();
            let init_data = parsed.take_init_data();
            let metadata = parsed.take_metadata();
            let message_events = parsed.take_message_events();
            timing.extract_fields = extract_fields_start.elapsed();

            let validate_filters_start = Instant::now();
            let details = details.ok_or_else(|| {
                ReplayBaseParseError::InvalidReplayData("missing replay.details".to_string())
            })?;
            let init_data = init_data.ok_or_else(|| {
                ReplayBaseParseError::InvalidReplayData("missing replay.initData".to_string())
            })?;
            let metadata = metadata.ok_or_else(|| {
                ReplayBaseParseError::InvalidReplayData(
                    "missing replay.gamemetadata.json".to_string(),
                )
            })?;

            if options.filters.only_blizzard && !details.m_isBlizzardMap {
                timing.validate_filters = validate_filters_start.elapsed();
                return Ok(None);
            }

            let disable_recover = details.m_disableRecoverGame.unwrap_or(false);
            if options.filters.require_recover_disabled && !disable_recover {
                timing.validate_filters = validate_filters_start.elapsed();
                return Ok(None);
            }
            timing.validate_filters = validate_filters_start.elapsed();

            let resolve_build_start = Instant::now();
            let replay_build = i64::from(base_build);
            let latest_build = i64::from(
                protocol_store
                    .latest()
                    .map_err(|error| ReplayBaseParseError::ProtocolStore(error.to_string()))?
                    .build(),
            );
            let selected_build = if protocol_store.build(base_build).is_ok() {
                replay_build
            } else {
                protocol_store
                    .closest_build(base_build)
                    .map(i64::from)
                    .unwrap_or(latest_build)
            };
            let build = ReplayBuildInfo::new(
                base_build,
                DetailedReplayAnalyzer::resolve_protocol_build(
                    replay_build,
                    latest_build,
                    selected_build,
                ),
            );
            timing.resolve_build = resolve_build_start.elapsed();

            let map_lookup_start = Instant::now();
            let map_title = if metadata.Title.is_empty() {
                "Unknown map".to_string()
            } else {
                metadata.Title.clone()
            };
            let map_name = inputs
                .map_names
                .get(&map_title)
                .and_then(|row| row.get("EN"))
                .cloned()
                .unwrap_or(map_title);
            timing.map_lookup = map_lookup_start.elapsed();

            let lobby_metadata_start = Instant::now();
            let extension = init_data
                .m_syncLobbyState
                .m_gameDescription
                .m_hasExtensionMod;
            let brutal_plus = init_data
                .m_syncLobbyState
                .m_lobbyState
                .m_slots
                .first()
                .map(|value| value.m_brutalPlusDifficulty as u32)
                .unwrap_or_default();
            timing.lobby_metadata = lobby_metadata_start.elapsed();

            let length_events_start = Instant::now();
            let length_numeric = ReplayNumericValue::Float(metadata.Duration);
            let start_time = detailed_event_collection
                .as_ref()
                .map(|collection| collection.start_time)
                .unwrap_or(ReplayNumericValue::Int(0));
            let last_deselect_event = detailed_event_collection
                .as_ref()
                .and_then(|collection| collection.last_deselect_event)
                .unwrap_or(ReplayNumericValue::Float(metadata.Duration));

            let metadata_players = &metadata.Players;
            if metadata_players.is_empty() {
                return Err(ReplayBaseParseError::InvalidReplayData(
                    "metadata Players must be array".to_string(),
                ));
            }

            let player0_result = metadata_players
                .first()
                .map(|value| value.Result.clone())
                .unwrap_or_default();
            let player1_result = metadata_players
                .get(1)
                .map(|value| value.Result.clone())
                .unwrap_or_default();
            let result = if player0_result == "Win" || player1_result == "Win" {
                "Victory".to_string()
            } else {
                "Defeat".to_string()
            };

            let accurate_length_numeric = if result == "Victory" && options.include_events {
                last_deselect_event.subtract(&start_time)
            } else {
                length_numeric.subtract(&start_time)
            };
            let accurate_length = accurate_length_numeric.as_f64();
            let realtime_length = DetailedReplayAnalyzer::realtime_length_from_replay(
                accurate_length,
                &details,
                &init_data,
            );
            let end_time = if result == "Victory" && options.include_events {
                last_deselect_event.as_f64()
            } else {
                metadata.Duration
            };
            let form_alength = DetailedReplayAnalyzer::format_duration(accurate_length);
            let length = CacheOverallStatsFile::duration_to_u64(length_numeric.as_f64());
            timing.length_events = length_events_start.elapsed();

            let identify_mutators_start = Instant::now();
            let mutator_context = ReplayMutatorParseContext::from_init_data(&init_data);
            let (mutators, weekly) = DetailedReplayAnalyzer::identify_mutators_for_replay(
                ReplayMutatorIdentificationInput {
                    event_collection: detailed_event_collection.as_ref(),
                    mutators_all: inputs.mutators_all,
                    mutators_ui: inputs.mutators_ui,
                    mutator_ids: inputs.mutator_ids,
                    cached_mutators: inputs.cached_mutators,
                    extension,
                    mm: is_mm_replay,
                    mutator_context: Some(&mutator_context),
                },
            );
            timing.identify_mutators = identify_mutators_start.elapsed();

            let collect_messages_start = Instant::now();
            let raw_messages = message_events
                .iter()
                .filter_map(ParsedReplayMessage::from_message_event)
                .collect::<Vec<ParsedReplayMessage>>();
            timing.collect_messages = collect_messages_start.elapsed();

            let hash_file_start = Instant::now();
            let hash = DetailedReplayAnalyzer::calculate_replay_hash(replay_path);
            timing.hash_file = hash_file_start.elapsed();

            let file_date_start = Instant::now();
            let date = DetailedReplayAnalyzer::file_date_string(replay_path).map_err(|error| {
                ReplayBaseParseError::IoRead {
                    path: replay_path.to_path_buf(),
                    message: error.to_string(),
                }
            })?;
            timing.file_date = file_date_start.elapsed();

            let detailed_event_filter_start = Instant::now();
            let detailed = if let Some(collection) = detailed_event_collection {
                debug_assert_eq!(collection.events.len(), collection.event_kinds.len());
                Some(ReplayDetailedParseContext {
                    events: collection.events,
                    event_kinds: collection.event_kinds,
                    start_time: start_time.as_f64(),
                    end_time,
                })
            } else {
                None
            };
            timing.detailed_event_filter = detailed_event_filter_start.elapsed();

            let build_base_start = Instant::now();
            let base = ReplayBaseParse {
                context: ReplayParsedContext {
                    details,
                    init_data,
                    metadata,
                },
                build,
                file: replay_path.display().to_string(),
                map_name,
                extension,
                brutal_plus,
                result,
                accurate_length,
                accurate_length_force_float: matches!(
                    accurate_length_numeric,
                    ReplayNumericValue::Float(_)
                ),
                realtime_length,
                form_alength,
                length,
                mutators,
                weekly,
                raw_messages,
                hash,
                date,
                detailed,
            };
            timing.build_base = build_base_start.elapsed();

            Ok(Some(base))
        })();
        (result, timing.finish(total_start.elapsed()))
    }

    fn resolve_protocol_build(
        replay_build: i64,
        latest_build: i64,
        selected_build: i64,
    ) -> ProtocolBuildValue {
        if let Some(mapped) = DetailedReplayAnalyzer::valid_protocol_mapping(replay_build) {
            if DetailedReplayAnalyzer::supported_legacy_protocol(mapped) {
                ProtocolBuildValue::Int(mapped as u32)
            } else {
                ProtocolBuildValue::Str(latest_build.to_string())
            }
        } else if replay_build == selected_build {
            ProtocolBuildValue::Int(replay_build as u32)
        } else {
            ProtocolBuildValue::Str(latest_build.to_string())
        }
    }

    fn collect_user_leave_times(context: &ReplayDetailedParseContext) -> IndexMap<i64, f64> {
        let mut user_leave_times = IndexMap::new();
        for (event, event_kind) in context.events.iter().zip(context.event_kinds.iter()) {
            if *event_kind != ReplayEventKind::GameUserLeave {
                continue;
            }
            let user = DetailedReplayAnalyzer::event_user_id(event)
                .map(|value| value + 1)
                .unwrap_or_default();
            let leave_time = DetailedReplayAnalyzer::event_gameloop(event) as f64 / 16.0;
            user_leave_times.insert(user, leave_time);
        }
        user_leave_times
    }

    fn file_date_string(file: &Path) -> Result<String, std::io::Error> {
        let modified = fs::metadata(file)?.modified()?;
        let datetime: DateTime<Utc> = DateTime::from(modified);
        Ok(datetime.format("%Y:%m:%d:%H:%M:%S").to_string())
    }

    pub(super) fn file_modified_seconds(file: &Path) -> Option<u64> {
        fs::metadata(file)
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs())
    }

    pub(super) fn calculate_replay_hash(path: &Path) -> String {
        Self::calculate_replay_file_digest(path).hash
    }

    pub(super) fn calculate_replay_file_digest(path: &Path) -> ReplayFileDigest {
        match fs::read(path) {
            Ok(bytes) => ReplayFileDigest {
                hash: format!("{:x}", md5::compute(&bytes)),
                size_bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            },
            Err(_) => ReplayFileDigest {
                hash: format!("{:x}", md5::compute(path.to_string_lossy().as_bytes())),
                size_bytes: fs::metadata(path)
                    .map(|metadata| metadata.len())
                    .unwrap_or(0),
            },
        }
    }

    fn parse_masteries(values: &[u32]) -> [u32; 6] {
        let mut out = [0_u32; 6];
        for (index, value) in values.iter().take(6).enumerate() {
            out[index] = *value;
        }
        out
    }

    pub(super) fn event_name(event: &ReplayEvent) -> &str {
        event._event()
    }
}

impl ReplayParsedInputBundle {
    fn parser_players_from_all_players(players: &[ParsedReplayPlayer]) -> Vec<ParsedReplayPlayer> {
        ParsedReplayPlayer::normalize_slots(players, true, Some(2))
    }

    fn normalized_cache_players(&self) -> Vec<ParsedReplayPlayer> {
        ParsedReplayPlayer::normalize_slots(&self.all_players, true, None)
    }

    fn normalized_cache_messages(&self) -> Vec<ParsedReplayMessage> {
        let user_leave_times = self
            .detailed
            .as_ref()
            .map(DetailedReplayAnalyzer::collect_user_leave_times)
            .unwrap_or_default();
        ParsedReplayMessage::sorted_with_leave_events(&self.parser.messages, &user_leave_times)
    }

    pub(super) fn cache_entry(&self) -> CacheReplayEntry {
        CacheReplayEntry::from_parsed_bundle(self)
    }

    fn supports_cache_filters(&self, filters: ReplayBaseParseFilters) -> bool {
        if filters.only_blizzard
            && (self.cache_context.is_mm_replay || !self.cache_context.is_blizzard_map)
        {
            return false;
        }

        if filters.require_recover_disabled && !self.cache_context.recover_disabled {
            return false;
        }

        true
    }

    fn is_cache_candidate(&self, filters: ReplayBaseParseFilters) -> bool {
        self.supports_cache_filters(filters)
            && self.parser.accurate_length != 0.0
            && (!filters.only_blizzard || self.commander_found)
    }

    pub(super) fn is_saved_cache_candidate(&self) -> bool {
        self.is_cache_candidate(ReplayBaseParseFilters::saved_cache())
    }

    fn from_base_parse(
        base: ReplayBaseParse,
        dictionaries: CacheGenerationData<'_>,
    ) -> Result<Self, ReplayBaseParseError> {
        let details = &base.context.details;
        let init_data = &base.context.init_data;
        let metadata = &base.context.metadata;
        let player_list = if details.m_playerList.is_empty() {
            Err(ReplayBaseParseError::InvalidReplayData(
                "details player list must be array".to_string(),
            ))
        } else {
            Ok(&details.m_playerList)
        }?;

        let length = metadata.Duration;
        let accurate_length = base.accurate_length;
        let cache_context = ReplayCacheContext {
            is_mm_replay: DetailedReplayAnalyzer::is_mm_replay_file(&base.file),
            is_blizzard_map: details.m_isBlizzardMap,
            recover_disabled: details.m_disableRecoverGame.unwrap_or(false),
        };

        let mut all_players = metadata
            .Players
            .iter()
            .enumerate()
            .map(|(index, player)| {
                let pid = (index + 1) as u8;
                let apm = if accurate_length == 0.0 {
                    0
                } else {
                    (player.APM * length / accurate_length).round_ties_even() as u32
                };
                ParsedReplayPlayer {
                    pid,
                    apm,
                    result: player.Result.clone(),
                    ..ParsedReplayPlayer::empty(pid)
                }
            })
            .collect::<Vec<_>>();

        let mut region = String::new();
        for (index, player) in player_list.iter().enumerate() {
            let Some(target) = all_players.get_mut(index) else {
                continue;
            };
            target.name = player.m_name.clone();
            target.race = player.m_race.clone();
            target.observer = player.m_observe != 0;

            if index == 0 {
                let region_code = player
                    .m_toon
                    .as_ref()
                    .map(|value| value.m_region)
                    .unwrap_or_default();
                region = DetailedReplayAnalyzer::region_name(region_code).to_string();
            }
        }

        let slots = &init_data.m_syncLobbyState.m_lobbyState.m_slots;
        let mut commander_found = false;
        for (index, slot) in slots.iter().enumerate() {
            let Some(target) = all_players.get_mut(index) else {
                continue;
            };
            let commander = slot.m_commander.clone();
            let commander_level = slot.m_commanderLevel;
            let commander_mastery_level = slot.m_commanderMasteryLevel;
            let prestige = slot.m_selectedCommanderPrestige;
            target.commander = commander.clone();
            target.commander_level = commander_level as u32;
            target.commander_mastery_level = commander_mastery_level as u32;
            target.prestige = prestige as u32;
            target.prestige_name = dictionaries
                .prestige_names
                .get(&commander)
                .and_then(|row| row.get(&prestige))
                .cloned()
                .unwrap_or_default();
            target.handle = slot.m_toonHandle.clone();
            target.masteries =
                DetailedReplayAnalyzer::parse_masteries(&slot.m_commanderMasteryTalents);

            if !commander.is_empty() {
                commander_found = true;
            }
        }

        let user_initial = &init_data.m_syncLobbyState.m_userInitialData;
        for (index, user) in user_initial.iter().enumerate() {
            let Some(target) = all_players.get_mut(index) else {
                continue;
            };
            let user_name = user.m_name.clone();
            if !user_name.is_empty() {
                target.name = user_name;
            }
        }

        let enemy_race_present = all_players.get(2).is_some();
        let enemy_race = all_players
            .get(2)
            .map(|player| player.race.clone())
            .unwrap_or_default();

        let difficulty_from_slot =
            |index: usize| -> Option<i64> { slots.get(index).map(|slot| slot.m_difficulty) };
        let mut diff_1_code = difficulty_from_slot(2);
        let mut diff_2_code = difficulty_from_slot(3);
        if diff_1_code.is_none() {
            diff_1_code = difficulty_from_slot(0).or_else(|| difficulty_from_slot(1));
        }
        if diff_2_code.is_none() {
            diff_2_code = difficulty_from_slot(1);
        }
        let diff_1_name =
            DetailedReplayAnalyzer::difficulty_name(diff_1_code.unwrap_or(4)).to_string();
        let diff_2_name =
            DetailedReplayAnalyzer::difficulty_name(diff_2_code.unwrap_or(4)).to_string();
        let ext_difficulty = if base.brutal_plus > 0 {
            format!("B+{}", base.brutal_plus)
        } else if diff_1_name == diff_2_name {
            diff_1_name.clone()
        } else {
            format!("{diff_1_name}/{diff_2_name}")
        };

        let parser = ParsedReplayInput {
            file: base.file,
            map_name: base.map_name,
            extension: base.extension,
            brutal_plus: base.brutal_plus,
            result: base.result,
            players: Self::parser_players_from_all_players(&all_players),
            difficulty: (diff_1_name, diff_2_name),
            accurate_length,
            form_alength: base.form_alength,
            length: base.length,
            mutators: base.mutators,
            weekly: base.weekly,
            messages: base.raw_messages,
            hash: Some(base.hash),
            build: base.build,
            date: base.date,
            enemy_race,
            ext_difficulty,
            region,
        };

        Ok(Self {
            parser,
            all_players,
            accurate_length_force_float: base.accurate_length_force_float,
            realtime_length: base.realtime_length,
            commander_found,
            enemy_race_present,
            cache_context,
            detailed: base.detailed,
        })
    }

    fn parse(
        replay_path: &Path,
        resources: &ReplayAnalysisResources,
        options: ReplayBaseParseOptions,
    ) -> Result<Option<Self>, ReplayBaseParseError> {
        let Some(base) = resources.parse_replay_base(replay_path, options)? else {
            return Ok(None);
        };

        Self::from_base_parse(base, resources.cache_generation_data()).map(Some)
    }

    pub(super) fn parse_detailed_required(
        replay_path: &Path,
        resources: &ReplayAnalysisResources,
    ) -> Result<Self, DetailedReplayAnalysisError> {
        Self::parse(
            replay_path,
            resources,
            ReplayBaseParseOptions {
                include_events: true,
                ..ReplayBaseParseOptions::default()
            },
        )
        .map_err(ReplayBaseParseError::into_detailed_analysis_error)?
        .ok_or_else(|| {
            DetailedReplayAnalysisError::InvalidReplayData(
                "detailed replay parsing unexpectedly skipped the replay".to_string(),
            )
        })
    }
}

impl CacheReplayEntry {
    pub fn parse_basic_with_resources(
        replay_path: &Path,
        resources: &ReplayAnalysisResources,
    ) -> Option<Self> {
        Self::parse_with_options(
            replay_path,
            resources,
            ReplayBaseParseOptions {
                include_events: false,
                filters: ReplayBaseParseFilters::saved_cache(),
            },
        )
        .map(|(entry, _)| entry)
    }

    fn from_parsed_bundle(parsed: &ReplayParsedInputBundle) -> Self {
        let players = parsed.normalized_cache_players();
        let messages = parsed.normalized_cache_messages();
        Self::from_parser_projection(
            &parsed.parser,
            &players,
            &messages,
            parsed.accurate_length_force_float,
            parsed.enemy_race_present,
            false,
        )
    }

    fn parse_with_options(
        replay_path: &Path,
        resources: &ReplayAnalysisResources,
        options: ReplayBaseParseOptions,
    ) -> Option<(Self, ReplayParsedInputBundle)> {
        Self::parse_with_options_timed(replay_path, resources, options)
            .into_parts()
            .0
    }

    pub(super) fn parse_with_options_timed(
        replay_path: &Path,
        resources: &ReplayAnalysisResources,
        options: ReplayBaseParseOptions,
    ) -> TimedReplayEntryParse {
        let total_start = Instant::now();
        let mut timing = ReplayEntryParseTiming::default();
        let (base_result, base_timing) = resources.parse_replay_base_timed(replay_path, options);
        timing.base = base_timing;

        let Some(base) = base_result.ok().flatten() else {
            return TimedReplayEntryParse::new(None, timing.finish(total_start.elapsed()));
        };

        let bundle_projection_start = Instant::now();
        let parsed =
            ReplayParsedInputBundle::from_base_parse(base, resources.cache_generation_data());
        timing.bundle_projection = bundle_projection_start.elapsed();
        let Ok(parsed) = parsed else {
            return TimedReplayEntryParse::new(None, timing.finish(total_start.elapsed()));
        };

        let candidate_filter_start = Instant::now();
        let is_cache_candidate = parsed.is_cache_candidate(options.filters);
        timing.candidate_filter = candidate_filter_start.elapsed();
        if !is_cache_candidate {
            return TimedReplayEntryParse::new(None, timing.finish(total_start.elapsed()));
        }

        let cache_entry_projection_start = Instant::now();
        let entry = parsed.cache_entry();
        timing.cache_entry_projection = cache_entry_projection_start.elapsed();
        TimedReplayEntryParse::new(Some((entry, parsed)), timing.finish(total_start.elapsed()))
    }

    pub(super) fn refreshed_for_candidate(&self, path: &Path, hash: &str) -> Self {
        let mut reused_entry = self.clone();
        reused_entry.file = CacheOverallStatsFile::normalized_path_string(path);
        reused_entry.hash = hash.to_string();
        reused_entry
    }
}
