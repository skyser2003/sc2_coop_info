mod report_helpers;
use super::*;

impl DetailedReplayAnalyzer {
    pub fn analyze_single_detailed(
        replay_path: &Path,
        main_player_handles: &HashSet<String>,
        resources: &ReplayAnalysisResources,
    ) -> Result<DetailedReplayAnalysisResult, DetailedReplayAnalysisError> {
        let parsed = ReplayParsedInputBundle::parse_detailed_required(replay_path, resources)?;
        DetailedReplayAnalyzer::analyze_parsed_replay_with_cache_entry(
            parsed,
            main_player_handles,
            resources.hidden_created_lost(),
            None,
            resources,
            AnalyzerTimingConfig::enabled_from_env(),
        )
    }
}

impl DetailedReplayAnalyzer {
    pub(super) fn build_stats_counter_dictionaries(
        dictionaries: &CacheGenerationData<'_>,
    ) -> StatsCounterDictionaries {
        StatsCounterDictionaries::new(
            dictionaries.unit_base_costs.clone(),
            dictionaries.royal_guards.clone(),
            dictionaries.horners_units.clone(),
            dictionaries.tychus_base_upgrades.clone(),
            dictionaries.tychus_ultimate_upgrades.clone(),
            dictionaries.outlaws.clone(),
        )
    }

    pub(super) fn analyze_parsed_replay_with_cache_entry(
        parsed: ReplayParsedInputBundle,
        main_player_handles: &HashSet<String>,
        hidden_created_lost: &HashSet<String>,
        basic_cache_entry: Option<&CacheReplayEntry>,
        resources: &ReplayAnalysisResources,
        collect_detailed_report_timings: bool,
    ) -> Result<DetailedReplayAnalysisResult, DetailedReplayAnalysisError> {
        let dictionaries = resources.cache_generation_data();
        let fallback_basic = match basic_cache_entry {
            Some(entry) => Cow::Borrowed(entry),
            None => Cow::Owned(parsed.cache_entry()),
        };
        let cache_persistable = parsed.is_saved_cache_candidate();
        let timed_report = DetailedReplayAnalyzer::analyze_replay_file_impl(
            main_player_handles,
            parsed,
            &dictionaries,
            resources.analysis_sets(),
            resources.stats_counter_dictionaries(),
            collect_detailed_report_timings,
        )?;
        let (report, detailed_report_timing) = timed_report.into_parts();
        let report_to_cache_entry_start = Instant::now();
        let cache_entry = CacheReplayEntry::from_report_with_basic(
            &report,
            Some(fallback_basic.as_ref()),
            hidden_created_lost,
        );
        let report_to_cache_entry = report_to_cache_entry_start.elapsed();

        Ok(DetailedReplayAnalysisResult::new(
            report,
            cache_entry,
            cache_persistable,
            detailed_report_timing,
            report_to_cache_entry,
        ))
    }

    fn analyze_replay_file_impl(
        main_player_handles: &HashSet<String>,
        parsed: ReplayParsedInputBundle,
        dictionaries: &CacheGenerationData<'_>,
        analysis_sets: &ReplayAnalysisSets,
        counter_dicts: Arc<StatsCounterDictionaries>,
        collect_detailed_report_timings: bool,
    ) -> Result<TimedDetailedReplayReport, DetailedReplayAnalysisError> {
        if collect_detailed_report_timings {
            Self::analyze_replay_file_impl_with_timings::<ReplayAnalysisTimingCollector>(
                main_player_handles,
                parsed,
                dictionaries,
                analysis_sets,
                counter_dicts,
            )
        } else {
            Self::analyze_replay_file_impl_with_timings::<ReplayAnalysisNoopTimingCollector>(
                main_player_handles,
                parsed,
                dictionaries,
                analysis_sets,
                counter_dicts,
            )
        }
    }

    fn analyze_replay_file_impl_with_timings<Timing: ReplayAnalysisTiming>(
        main_player_handles: &HashSet<String>,
        parsed: ReplayParsedInputBundle,
        dictionaries: &CacheGenerationData<'_>,
        analysis_sets: &ReplayAnalysisSets,
        counter_dicts: Arc<StatsCounterDictionaries>,
    ) -> Result<TimedDetailedReplayReport, DetailedReplayAnalysisError> {
        let ReplayParsedInputBundle {
            mut parser,
            realtime_length,
            detailed,
            ..
        } = parsed;
        let ReplayDetailedParseContext {
            events,
            event_kinds,
            start_time,
            end_time,
        } = detailed.ok_or_else(|| {
            DetailedReplayAnalysisError::InvalidReplayData(
                "detailed replay parsing did not include event context".to_string(),
            )
        })?;
        let mut timings = Timing::new(parser.file.as_str());
        timings.add_events_input_count(events.len());
        let setup_started = timings.start();

        let main_player = i64::from(parser.selected_main_player_pid(main_player_handles));
        let ally_player = if main_player == 2 { 1 } else { 2 };

        let main_player_row = parser.player(main_player as u8);
        let ally_player_row = parser.player(ally_player as u8);
        let main_commander = main_player_row
            .map(|player| player.commander.clone())
            .filter(|value| !value.is_empty());
        let ally_commander = ally_player_row
            .map(|player| player.commander.clone())
            .filter(|value| !value.is_empty());
        let main_masteries = main_player_row
            .map(|player| player.masteries)
            .unwrap_or([0_u32; 6]);
        let ally_masteries = ally_player_row
            .map(|player| player.masteries)
            .unwrap_or([0_u32; 6]);

        let mut vespene_drone_identifier =
            ReplayDroneIdentifierCore::new(main_commander.clone(), ally_commander.clone());
        let mut main_stats_counter =
            ReplayStatsCounterCore::new(counter_dicts.clone(), main_masteries, main_commander);
        let mut ally_stats_counter =
            ReplayStatsCounterCore::new(counter_dicts, ally_masteries, ally_commander);
        if DetailedReplayAnalyzer::is_mm_replay_file(&parser.file) {
            main_stats_counter.set_enable_updates(true);
            ally_stats_counter.set_enable_updates(true);
        }

        let do_not_count_kills_set = &analysis_sets.do_not_count_kills;
        let duplicating_units_set = &analysis_sets.duplicating_units;
        let dont_count_morphs_set = &analysis_sets.dont_count_morphs;
        let self_killing_units_set = &analysis_sets.self_killing_units;
        let aoe_units_set = &analysis_sets.aoe_units;
        let tychus_outlaws_set = &analysis_sets.tychus_outlaws;
        let units_killed_in_morph_set = &analysis_sets.units_killed_in_morph;
        let salvage_units_set = &analysis_sets.salvage_units;
        let unit_add_losses_to_set = &analysis_sets.unit_add_losses_to;
        let commander_no_units_values_set = &analysis_sets.commander_no_units_values;

        let mut amon_player_ids_set = ReplayPlayerIdSet::from_values([3_i64, 4_i64]);
        for (mission_name, player_ids) in dictionaries.amon_player_ids.iter() {
            if !DetailedReplayAnalyzer::map_name_has_amon_override(&parser.map_name, mission_name) {
                continue;
            }
            amon_player_ids_set.extend(player_ids.iter().copied());
            break;
        }
        let map_flags = ReplayMapAnalysisFlags::new(parser.map_name.as_str());
        let event_string_sets = &analysis_sets.event_string_sets;

        let mut unit_type_dict_main: UnitTypeCountMap = IndexMap::new();
        let mut unit_type_dict_ally: UnitTypeCountMap = IndexMap::new();
        let mut unit_type_dict_amon: UnitTypeCountMap = IndexMap::new();
        let mut unit_dict: UnitStateMap = HashMap::new();
        let mut dt_ht_ignore = vec![0_i64; 17];
        let mut killcounts = vec![0_i64; 18];
        let mut commander_by_player = HashMap::<i64, String>::new();
        let mut mastery_by_player = HashMap::from([(1_i64, [0_i64; 6]), (2_i64, [0_i64; 6])]);
        let mut prestige_by_player = HashMap::<i64, String>::new();
        let mut outlaw_order: Vec<String> = Vec::new();
        let mut outlaw_order_seen: HashSet<String> = HashSet::new();
        let mut wave_units = WaveUnitsState::default();
        let mut identified_waves: IdentifiedWavesMap = BTreeMap::new();
        let mut startup_removed_wave_units = HashSet::<String>::new();
        let mut killbot_feed = vec![0_i64, 0, 0];
        let mut custom_kill_count: replay_event_handlers::NestedPlayerCountMap = IndexMap::new();
        let mut used_mutator_spider_mines: HashSet<i64> = HashSet::new();
        let mut bonus_timings: Vec<f64> = Vec::new();
        let mut research_vessel_landed_timing: Option<i64> = None;
        let mut unit_id = 0_i64;
        let mut last_biomass_position = [0_i64, 0, 0];
        let mut abathur_kill_locusts = HashSet::new();
        let mut mutator_dehaka_drag_unit_ids = HashSet::new();
        let mut mw_bonus_initial_timing = [0.0_f64, 0.0_f64];
        let mut murvar_spawns = HashSet::new();
        let mut glevig_spawns = HashSet::new();
        let mut broodlord_broodlings = HashSet::new();
        let mut user_leave_times: IndexMap<i64, f64> = IndexMap::new();
        let mut mind_controlled_units = HashSet::new();
        let mut zagaras_dummy_zerglings = HashSet::new();
        let mut unit_killed_by: replay_event_handlers::TextListMapping = IndexMap::new();
        let mut ally_kills_counted_toward_main = 0_i64;
        let mut last_aoe_unit_killed: Vec<Option<(String, f64)>> = vec![None; 17];
        let mut main_icons_base = BTreeMap::<String, u64>::new();
        let mut ally_icons_base = BTreeMap::<String, u64>::new();
        timings.finish(ReplayReportTimingSpan::Setup, setup_started);

        let end_gameloop = end_time * 16.0;
        let ally_leave_transfer_threshold = end_time * 0.5;
        let mut ally_kills_transfer_to_main = false;
        let event_loop_started = timings.start();
        for (event, current_event_kind) in events.iter().zip(event_kinds.iter().copied()) {
            timings.increment_event_kind(current_event_kind);
            let event_gameloop = DetailedReplayAnalyzer::event_gameloop(event);

            if current_event_kind == ReplayEventKind::GameUserLeave {
                let handler_started = timings.start();
                let user_id = DetailedReplayAnalyzer::event_user_id(event).unwrap_or_default();
                let leaving_player = user_id + 1;
                let leave_time = event_gameloop as f64 / 16.0;
                ReplayEventHandlers::replay_handle_game_user_leave_event_fields(
                    user_id,
                    event_gameloop as f64,
                    &mut user_leave_times,
                );
                if leaving_player == ally_player && leave_time < ally_leave_transfer_threshold {
                    ally_kills_transfer_to_main = true;
                }
                timings.finish(ReplayReportTimingSpan::EventGameUserLeave, handler_started);
                continue;
            }

            if event_gameloop as f64 > end_gameloop {
                continue;
            }

            match current_event_kind {
                ReplayEventKind::GameCommand | ReplayEventKind::GameCommandUpdateTargetUnit => {
                    let ReplayEvent::Game(game_event) = event else {
                        continue;
                    };
                    let drone_event_kind = match current_event_kind {
                        ReplayEventKind::GameCommand => ReplayDroneCommandEventKind::Command,
                        ReplayEventKind::GameCommandUpdateTargetUnit => {
                            ReplayDroneCommandEventKind::CommandUpdateTargetUnit
                        }
                        _ => unreachable!(),
                    };
                    let handler_started = timings.start();
                    vespene_drone_identifier.event(drone_event_kind, game_event);
                    timings.finish(ReplayReportTimingSpan::EventDroneCommand, handler_started);
                }
                ReplayEventKind::TrackerPlayerStats => {
                    let ReplayEvent::Tracker(event) = event else {
                        continue;
                    };
                    let handler_started = timings.start();
                    let player = event.m_player_id.unwrap_or_default();
                    if let Some(stats) = event.m_stats.as_ref() {
                        let supply_used =
                            stats.m_score_value_food_used.unwrap_or_default() / 4096.0;
                        let collection_rate = stats
                            .m_score_value_minerals_collection_rate
                            .unwrap_or_default()
                            + stats
                                .m_score_value_vespene_collection_rate
                                .unwrap_or_default();

                        if let Some(update) =
                            ReplayEventHandlers::replay_handle_player_stats_event_fields(
                                player,
                                main_player,
                                ally_player,
                                supply_used,
                                collection_rate,
                                &killcounts,
                            )
                        {
                            match update.target() {
                                StatsCounterTarget::Main => {
                                    main_stats_counter.add_stats(
                                        &unit_type_dict_main,
                                        &vespene_drone_identifier,
                                        update.kills(),
                                        update.supply_used(),
                                        update.collection_rate(),
                                    );
                                }
                                StatsCounterTarget::Ally => {
                                    ally_stats_counter.add_stats(
                                        &unit_type_dict_ally,
                                        &vespene_drone_identifier,
                                        update.kills(),
                                        update.supply_used(),
                                        update.collection_rate(),
                                    );
                                }
                            }
                        }
                    }
                    timings.finish(ReplayReportTimingSpan::EventPlayerStats, handler_started);
                }
                ReplayEventKind::TrackerUpgrade => {
                    let ReplayEvent::Tracker(event) = event else {
                        continue;
                    };
                    if !matches!(event.m_player_id, Some(1 | 2)) {
                        continue;
                    }
                    let handler_started = timings.start();
                    let upg_name = event.m_upgrade_type_name.clone().unwrap_or_default();
                    let upg_pid = event.m_player_id.unwrap_or_default();
                    let upgrade_count = event.m_count.unwrap_or_default();
                    let update = ReplayEventHandlers::replay_handle_upgrade_event_fields(
                        UpgradeEventHandlerInput {
                            upg_name: upg_name.as_str(),
                            upg_pid,
                            upgrade_count,
                            main_player,
                            ally_player,
                            commander_upgrades: &dictionaries
                                .replay_analysis_data
                                .commander_upgrades,
                            mastery_upgrade_indices: &analysis_sets.mastery_upgrade_indices,
                            prestige_upgrade_names: &analysis_sets.prestige_upgrade_names,
                        },
                    );

                    if let Some(target) = update.target() {
                        match target {
                            StatsCounterTarget::Main => {
                                main_stats_counter.upgrade_event(upg_name.as_str())
                            }
                            StatsCounterTarget::Ally => {
                                ally_stats_counter.upgrade_event(upg_name.as_str())
                            }
                        }
                    }

                    if let Some(commander_name) = update.commander_name() {
                        commander_by_player.insert(upg_pid, commander_name.to_string());
                        vespene_drone_identifier.update_commanders(upg_pid, commander_name);

                        if let Some(target) = update.target() {
                            match target {
                                StatsCounterTarget::Main => {
                                    main_stats_counter.update_commander(commander_name);
                                }
                                StatsCounterTarget::Ally => {
                                    ally_stats_counter.update_commander(commander_name);
                                }
                            }
                        }
                    }

                    if let Some(mastery_idx) = update.mastery_index() {
                        if let Some(row) = mastery_by_player.get_mut(&upg_pid)
                            && let Ok(index) = usize::try_from(mastery_idx)
                            && index < row.len()
                        {
                            row[index] = update.upgrade_count();
                        }

                        if let Some(target) = update.target() {
                            match target {
                                StatsCounterTarget::Main => {
                                    main_stats_counter
                                        .update_mastery(mastery_idx, update.upgrade_count());
                                }
                                StatsCounterTarget::Ally => {
                                    ally_stats_counter
                                        .update_mastery(mastery_idx, update.upgrade_count());
                                }
                            }
                        }
                    }

                    if let Some(prestige_name) = update.prestige_name() {
                        prestige_by_player.insert(upg_pid, prestige_name.to_string());
                        if let Some(target) = update.target() {
                            match target {
                                StatsCounterTarget::Main => {
                                    main_stats_counter.update_prestige(prestige_name);
                                }
                                StatsCounterTarget::Ally => {
                                    ally_stats_counter.update_prestige(prestige_name);
                                }
                            }
                        }
                    }
                    timings.finish(ReplayReportTimingSpan::EventUpgrade, handler_started);
                }
                ReplayEventKind::TrackerUnitBorn | ReplayEventKind::TrackerUnitInit => {
                    let ReplayEvent::Tracker(event) = event else {
                        continue;
                    };
                    let handler_started = timings.start();
                    let event_fields = UnitBornOrInitEventFields::new(
                        event.m_unit_type_name.as_deref().unwrap_or_default(),
                        event.m_creator_ability_name.as_deref(),
                        UnitBornOrInitUnitIds::new(
                            DetailedReplayAnalyzer::replay_event_unitid(event).unwrap_or_default(),
                            DetailedReplayAnalyzer::replay_creator_unitid(event),
                        ),
                        event.m_control_player_id.unwrap_or_default(),
                        event.game_loop,
                        UnitEventPosition::new(
                            event.m_x.unwrap_or_default(),
                            event.m_y.unwrap_or_default(),
                        ),
                    );
                    let update = ReplayEventHandlers::replay_handle_unit_born_or_init_event_fields(
                        UnitBornOrInitHandlerInput {
                            event: &event_fields,
                            main_player,
                            ally_player,
                            amon_players: &amon_player_ids_set,
                            unit_dict: &mut unit_dict,
                            start_time,
                            unit_type_dict_main: &mut unit_type_dict_main,
                            unit_type_dict_ally: &mut unit_type_dict_ally,
                            unit_type_dict_amon: &mut unit_type_dict_amon,
                            mutator_dehaka_drag_unit_ids: &mut mutator_dehaka_drag_unit_ids,
                            murvar_spawns: &mut murvar_spawns,
                            glevig_spawns: &mut glevig_spawns,
                            broodlord_broodlings: &mut broodlord_broodlings,
                            outlaw_order: &mut outlaw_order,
                            outlaw_order_seen: &mut outlaw_order_seen,
                            wave_units: &mut wave_units,
                            identified_waves: &mut identified_waves,
                            abathur_kill_locusts: &mut abathur_kill_locusts,
                            last_biomass_position,
                            revival_types: &dictionaries.replay_analysis_data.revival_types,
                            primal_combat_predecessors: &dictionaries
                                .replay_analysis_data
                                .primal_combat_predecessors,
                            tychus_outlaws: tychus_outlaws_set,
                            units_in_waves: dictionaries.units_in_waves,
                            string_sets: event_string_sets,
                        },
                    );
                    unit_id = update.unit_id();
                    last_biomass_position = update.last_biomass_position();

                    if let Some((target, unit_type)) = update.created_event() {
                        match target {
                            StatsCounterTarget::Main => {
                                main_stats_counter.unit_created_event(unit_type, event);
                            }
                            StatsCounterTarget::Ally => {
                                ally_stats_counter.unit_created_event(unit_type, event);
                            }
                        }
                    }
                    timings.finish(ReplayReportTimingSpan::EventUnitBornOrInit, handler_started);

                    if current_event_kind == ReplayEventKind::TrackerUnitInit {
                        let handler_started = timings.start();
                        if event.m_unit_type_name.as_deref() == Some("Archon") {
                            let control_pid = event.m_control_player_id.unwrap_or_default();
                            ReplayEventHandlers::replay_handle_archon_init_event_control_pid(
                                control_pid,
                                &mut dt_ht_ignore,
                            );
                        }
                        timings
                            .finish(ReplayReportTimingSpan::EventUnitInitArchon, handler_started);
                    }
                }
                ReplayEventKind::TrackerUnitTypeChange => {
                    let ReplayEvent::Tracker(event) = event else {
                        continue;
                    };
                    let event_unit_id_started = timings.start();
                    let event_unit_id = DetailedReplayAnalyzer::replay_event_unitid(event);
                    let event_unit_in_dict = event_unit_id
                        .map(|value| unit_dict.contains_key(&value))
                        .unwrap_or(false);
                    timings.finish(
                        ReplayReportTimingSpan::EventUnitIdLookup,
                        event_unit_id_started,
                    );
                    if !event_unit_in_dict {
                        continue;
                    }

                    let handler_started = timings.start();
                    let event_fields = UnitTypeChangeEventFields::new(
                        event_unit_id.unwrap_or_default(),
                        event.m_unit_type_name.as_deref().unwrap_or_default(),
                        event.game_loop,
                    );
                    let update = ReplayEventHandlers::replay_handle_unit_type_change_event_fields(
                        UnitTypeChangeHandlerInput {
                            event: &event_fields,
                            map_flags: &map_flags,
                            main_player,
                            ally_player,
                            amon_players: &amon_player_ids_set,
                            unit_dict: &mut unit_dict,
                            unit_type_dict_main: &mut unit_type_dict_main,
                            unit_type_dict_ally: &mut unit_type_dict_ally,
                            unit_type_dict_amon: &mut unit_type_dict_amon,
                            start_time,
                            bonus_timings: &mut bonus_timings,
                            legacy_spawn_filter_unit_id: unit_id,
                            glevig_spawns: &glevig_spawns,
                            murvar_spawns: &murvar_spawns,
                            zagaras_dummy_zerglings: &mut zagaras_dummy_zerglings,
                            broodlord_broodlings: &broodlord_broodlings,
                            research_vessel_landed_timing,
                            units_killed_in_morph: units_killed_in_morph_set,
                            unit_name_dict: dictionaries.unit_name_dict,
                            unit_add_losses_to: unit_add_losses_to_set,
                            dont_count_morphs: dont_count_morphs_set,
                            string_sets: event_string_sets,
                        },
                    );
                    research_vessel_landed_timing = update.landed_timing();

                    if let Some((target, new_unit, old_unit)) = update.unit_change_event() {
                        match target {
                            StatsCounterTarget::Main => {
                                main_stats_counter.unit_change_event(new_unit, old_unit);
                            }
                            StatsCounterTarget::Ally => {
                                ally_stats_counter.unit_change_event(new_unit, old_unit);
                            }
                        }
                    }
                    timings.finish(ReplayReportTimingSpan::EventUnitTypeChange, handler_started);
                }
                ReplayEventKind::TrackerUnitOwnerChange => {
                    let ReplayEvent::Tracker(event) = event else {
                        continue;
                    };
                    let event_unit_id_started = timings.start();
                    let event_unit_id = DetailedReplayAnalyzer::replay_event_unitid(event);
                    let event_unit_in_dict = event_unit_id
                        .map(|value| unit_dict.contains_key(&value))
                        .unwrap_or(false);
                    timings.finish(
                        ReplayReportTimingSpan::EventUnitIdLookup,
                        event_unit_id_started,
                    );
                    let Some(changed_unit_id) = event_unit_id.filter(|_| event_unit_in_dict) else {
                        continue;
                    };

                    let handler_started = timings.start();
                    let control_pid = event.m_control_player_id.unwrap_or_default();
                    let game_time = event.game_loop as f64 / 16.0 - start_time;
                    let update = ReplayEventHandlers::replay_handle_unit_owner_change_event_fields(
                        UnitOwnerChangeHandlerInput {
                            event_unit_id: changed_unit_id,
                            map_flags: &map_flags,
                            control_pid,
                            main_player,
                            ally_player,
                            amon_players: &amon_player_ids_set,
                            unit_dict: &mut unit_dict,
                            game_time,
                            bonus_timings: &mut bonus_timings,
                            mw_bonus_initial_timing: &mut mw_bonus_initial_timing,
                        },
                    );

                    if let Some(mindcontrolled_unit_id) = update.mind_controlled_unit_id() {
                        mind_controlled_units.insert(mindcontrolled_unit_id);
                        match update.icon_target() {
                            Some(StatsCounterTarget::Main) => {
                                DetailedReplayAnalyzer::increment_icon_count(
                                    &mut main_icons_base,
                                    "mc",
                                    1,
                                );
                            }
                            Some(StatsCounterTarget::Ally) => {
                                DetailedReplayAnalyzer::increment_icon_count(
                                    &mut ally_icons_base,
                                    "mc",
                                    1,
                                );
                            }
                            None => {}
                        }
                    }
                    timings.finish(
                        ReplayReportTimingSpan::EventUnitOwnerChange,
                        handler_started,
                    );
                }
                ReplayEventKind::TrackerUnitDied => {
                    let ReplayEvent::Tracker(event) = event else {
                        continue;
                    };
                    let event_unit_id_started = timings.start();
                    let event_unit_id = DetailedReplayAnalyzer::replay_event_unitid(event);
                    let killed_snapshot = event_unit_id.and_then(|value| unit_dict.get(&value));
                    let event_unit_in_dict = killed_snapshot.is_some();
                    timings.finish(
                        ReplayReportTimingSpan::EventUnitIdLookup,
                        event_unit_id_started,
                    );

                    let handler_started = timings.start();
                    if !event_unit_in_dict {
                        let killed_unit_type =
                            event.m_unit_type_name.as_deref().unwrap_or_default();
                        if !do_not_count_kills_set.contains(killed_unit_type)
                            && let Some(killer_player) = event.m_killer_player_id
                            && let Ok(index) = usize::try_from(killer_player)
                            && let Some(value) = killcounts.get_mut(index)
                        {
                            *value += 1;
                        }
                    }

                    ally_kills_counted_toward_main =
                        ReplayEventHandlers::replay_handle_unit_died_kill_stats_event_fields(
                            UnitDiedKillStatsHandlerInput {
                                killed_row: killed_snapshot,
                                killing_player: event.m_killer_player_id,
                                gameloop: event.game_loop,
                                main_player,
                                ally_player,
                                amon_players: &amon_player_ids_set,
                                killcounts: &mut killcounts,
                                ally_kills_transfer_to_main,
                                last_aoe_unit_killed: &mut last_aoe_unit_killed,
                                ally_kills_counted_toward_main,
                                do_not_count_kills: do_not_count_kills_set,
                                aoe_units: aoe_units_set,
                            },
                        );
                    timings.finish(
                        ReplayReportTimingSpan::EventUnitDiedKillStats,
                        handler_started,
                    );

                    let Some(detail_unit_id) = event_unit_id.filter(|_| event_unit_in_dict) else {
                        continue;
                    };
                    let Some(killed_snapshot) = killed_snapshot else {
                        continue;
                    };
                    let handler_started = timings.start();
                    let event_fields = UnitDiedEventFields::new(
                        detail_unit_id,
                        DetailedReplayAnalyzer::replay_killer_unitid(event),
                        event.m_killer_player_id,
                        event.game_loop,
                        event.m_x.unwrap_or_default(),
                        event.m_y.unwrap_or_default(),
                    );
                    let update = ReplayEventHandlers::replay_handle_unit_died_detail_event_fields(
                        UnitDiedDetailHandlerInput {
                            event: &event_fields,
                            killed_row: killed_snapshot,
                            map_flags: &map_flags,
                            main_player,
                            ally_player,
                            amon_players: &amon_player_ids_set,
                            unit_id,
                            unit_type_dict_main: &mut unit_type_dict_main,
                            unit_type_dict_ally: &mut unit_type_dict_ally,
                            unit_type_dict_amon: &mut unit_type_dict_amon,
                            unit_dict: &unit_dict,
                            dt_ht_ignore: &mut dt_ht_ignore,
                            start_time,
                            commander_by_player: &commander_by_player,
                            killbot_feed: &mut killbot_feed,
                            custom_kill_count: &mut custom_kill_count,
                            used_mutator_spider_mines: &mut used_mutator_spider_mines,
                            bonus_timings: &mut bonus_timings,
                            abathur_kill_locusts: &abathur_kill_locusts,
                            mutator_dehaka_drag_unit_ids: &mutator_dehaka_drag_unit_ids,
                            murvar_spawns: &murvar_spawns,
                            glevig_spawns: &glevig_spawns,
                            broodlord_broodlings: &broodlord_broodlings,
                            unit_killed_by: &mut unit_killed_by,
                            mind_controlled_units: &mind_controlled_units,
                            zagaras_dummy_zerglings: &zagaras_dummy_zerglings,
                            last_aoe_unit_killed: &last_aoe_unit_killed,
                            commander_no_units: &dictionaries
                                .replay_analysis_data
                                .commander_no_units,
                            commander_no_units_values: commander_no_units_values_set,
                            hfts_units: dictionaries.hfts_units,
                            tus_units: dictionaries.tus_units,
                            do_not_count_kills: do_not_count_kills_set,
                            self_killing_units: self_killing_units_set,
                            duplicating_units: duplicating_units_set,
                            salvage_units: salvage_units_set,
                            startup_removed_wave_units: &mut startup_removed_wave_units,
                            units_in_waves: dictionaries.units_in_waves,
                            string_sets: event_string_sets,
                        },
                    );
                    unit_id = update.current_unit_id();

                    if let Some((target, unit_name)) = update.salvaged_unit() {
                        match target {
                            StatsCounterTarget::Main => {
                                main_stats_counter.append_salvaged_unit(unit_name);
                            }
                            StatsCounterTarget::Ally => {
                                ally_stats_counter.append_salvaged_unit(unit_name);
                            }
                        }
                    }

                    if let Some((target, unit_name)) = update.mindcontrolled_unit_died() {
                        match target {
                            StatsCounterTarget::Main => {
                                main_stats_counter.mindcontrolled_unit_dies(unit_name);
                            }
                            StatsCounterTarget::Ally => {
                                ally_stats_counter.mindcontrolled_unit_dies(unit_name);
                            }
                        }
                    }
                    timings.finish(ReplayReportTimingSpan::EventUnitDiedDetail, handler_started);
                }
                _ => {}
            }
        }
        timings.finish(ReplayReportTimingSpan::EventsTotal, event_loop_started);

        let overrides_started = timings.start();
        parser.apply_player_overrides(
            &commander_by_player,
            &mastery_by_player,
            &prestige_by_player,
        );
        parser.messages =
            ParsedReplayMessage::sorted_with_leave_events(&parser.messages, &user_leave_times);
        timings.finish(
            ReplayReportTimingSpan::PostPlayerOverridesMessages,
            overrides_started,
        );

        let player_stats_started = timings.start();
        let main_name = parser
            .player(main_player as u8)
            .map(|player| player.name.clone())
            .unwrap_or_default();
        let ally_name = parser
            .player(ally_player as u8)
            .map(|player| player.name.clone())
            .unwrap_or_default();

        let mut player_stats = BTreeMap::<u8, AnalysisPlayerStatsSeries>::new();
        player_stats.insert(
            main_player as u8,
            main_stats_counter.get_stats(main_name.as_str()),
        );
        player_stats.insert(
            ally_player as u8,
            ally_stats_counter.get_stats(ally_name.as_str()),
        );
        timings.finish(
            ReplayReportTimingSpan::PostPlayerStats,
            player_stats_started,
        );

        let bonus_comp_started = timings.start();
        let bonus = bonus_timings
            .iter()
            .map(|value| DetailedReplayAnalyzer::format_mm_ss(*value))
            .collect::<Vec<String>>();
        let comp = DetailedReplayAnalyzer::enemy_comp_from_identified_waves(
            &identified_waves,
            dictionaries.unit_comp_dict,
        )
        .or_else(|| {
            DetailedReplayAnalyzer::enemy_comp_from_startup_removed_units(
                &startup_removed_wave_units,
                dictionaries.unit_comp_dict,
            )
        })
        .unwrap_or_else(|| "Unidentified AI".to_string());
        timings.finish(ReplayReportTimingSpan::PostBonusComp, bonus_comp_started);

        let custom_icons_started = timings.start();
        DetailedReplayAnalyzer::apply_custom_kill_icons(
            &mut main_icons_base,
            &mut ally_icons_base,
            &custom_kill_count,
            &unit_type_dict_amon,
            &map_flags,
            main_player,
            ally_player,
        );
        timings.finish(
            ReplayReportTimingSpan::PostCustomKillIcons,
            custom_icons_started,
        );

        let main_units_started = timings.start();
        let (main_units, mut main_icons) =
            DetailedReplayAnalyzer::fill_unit_kills_and_icons(FillUnitKillsAndIconsInput {
                base_icons: &main_icons_base,
                player: main_player,
                main_player,
                unit_counts: &unit_type_dict_main,
                ally_kills_counted_toward_main,
                killcounts: &killcounts,
                unit_name_dict: dictionaries.unit_name_dict,
                unit_add_kills_to: dictionaries.unit_add_kills_to,
                unit_add_losses_to: &dictionaries.replay_analysis_data.unit_add_losses_to,
                analysis_sets,
            });
        timings.finish(
            ReplayReportTimingSpan::PostMainUnitsIcons,
            main_units_started,
        );
        let ally_units_started = timings.start();
        let (ally_units, mut ally_icons) =
            DetailedReplayAnalyzer::fill_unit_kills_and_icons(FillUnitKillsAndIconsInput {
                base_icons: &ally_icons_base,
                player: ally_player,
                main_player,
                unit_counts: &unit_type_dict_ally,
                ally_kills_counted_toward_main,
                killcounts: &killcounts,
                unit_name_dict: dictionaries.unit_name_dict,
                unit_add_kills_to: dictionaries.unit_add_kills_to,
                unit_add_losses_to: &dictionaries.replay_analysis_data.unit_add_losses_to,
                analysis_sets,
            });
        timings.finish(
            ReplayReportTimingSpan::PostAllyUnitsIcons,
            ally_units_started,
        );

        let killbot_icons_started = timings.start();
        let main_killbot_feed = DetailedReplayAnalyzer::count_for_pid(&killbot_feed, main_player);
        if main_killbot_feed > 0 {
            DetailedReplayAnalyzer::set_icon_count(&mut main_icons, "killbots", main_killbot_feed);
        }
        let ally_killbot_feed = DetailedReplayAnalyzer::count_for_pid(&killbot_feed, ally_player);
        if ally_killbot_feed > 0 {
            DetailedReplayAnalyzer::set_icon_count(&mut ally_icons, "killbots", ally_killbot_feed);
        }
        timings.finish(
            ReplayReportTimingSpan::PostKillbotIcons,
            killbot_icons_started,
        );

        let amon_units_started = timings.start();
        let amon_units = DetailedReplayAnalyzer::fill_amon_units(
            &unit_type_dict_amon,
            &killcounts,
            &amon_player_ids_set,
            dictionaries.unit_name_dict,
            dictionaries.unit_add_kills_to,
            &dictionaries.replay_analysis_data.unit_add_losses_to,
            analysis_sets,
        );
        timings.finish(ReplayReportTimingSpan::PostAmonUnits, amon_units_started);

        let report_started = timings.start();
        let mut detailed_input = ReplayReportDetailedInput::from_parser(parser);
        detailed_input.positions = Some(PlayerPositions {
            main: main_player as u8,
            ally: ally_player as u8,
        });
        detailed_input.detail = Some(ReplayReportDetailData {
            length: realtime_length,
            bonus,
            comp,
            replay_hash: None,
            main_kills: DetailedReplayAnalyzer::clamp_nonnegative_to_u64(
                DetailedReplayAnalyzer::count_for_pid(&killcounts, main_player),
            ),
            ally_kills: DetailedReplayAnalyzer::clamp_nonnegative_to_u64(
                DetailedReplayAnalyzer::count_for_pid(&killcounts, ally_player),
            ),
            main_icons,
            ally_icons,
            main_units,
            ally_units,
            amon_units,
            player_stats,
            outlaw_order,
        });

        let report = ReplayReport::from_detailed_input(
            &detailed_input.parser.file,
            &detailed_input,
            main_player_handles,
        );
        timings.finish(ReplayReportTimingSpan::PostReportBuild, report_started);
        let detailed_report_timing = timings.breakdown();
        timings.print();
        Ok(TimedDetailedReplayReport::new(
            report,
            detailed_report_timing,
        ))
    }
}
