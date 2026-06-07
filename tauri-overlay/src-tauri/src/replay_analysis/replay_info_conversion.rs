use super::*;

impl ReplayAnalysisOps {
    fn report_player(report: &ReplayReport, pid: u8) -> Option<&ParsedReplayPlayer> {
        report
            .parser
            .players
            .iter()
            .find(|player| player.pid == pid)
    }
}

impl ReplayAnalysisOps {
    fn with_outlaw_icons(
        mut icons: Value,
        commander: &str,
        outlaw_order: Option<&Vec<String>>,
    ) -> Value {
        if commander != "Tychus" {
            return icons;
        }

        let Some(order) = outlaw_order else {
            return icons;
        };
        if order.is_empty() {
            return icons;
        }

        let Some(object) = icons.as_object_mut() else {
            return icons;
        };
        object.insert(
            "outlaws".to_string(),
            Value::Array(order.iter().cloned().map(Value::String).collect()),
        );
        icons
    }
}

impl ReplayAnalysisOps {
    fn cache_json_value<T: serde::Serialize>(value: &T) -> Value {
        serde_json::to_value(value).unwrap_or_else(|_| Value::Object(Default::default()))
    }
}

impl ReplayAnalysisOps {
    fn cache_player(entry: &CacheReplayEntry, pid: u8) -> Option<&CachePlayer> {
        entry.players.iter().find(|player| player.pid == pid)
    }
}

impl ReplayAnalysisOps {
    fn cache_player_text(
        player: Option<&CachePlayer>,
        select: impl Fn(&CachePlayer) -> Option<&String>,
    ) -> String {
        player.and_then(select).cloned().unwrap_or_default()
    }
}

impl ReplayAnalysisOps {
    fn cache_player_u64(
        player: Option<&CachePlayer>,
        select: impl Fn(&CachePlayer) -> Option<u64>,
    ) -> u64 {
        player.and_then(select).unwrap_or(0)
    }
}

impl ReplayAnalysisOps {
    fn cache_player_masteries(player: Option<&CachePlayer>) -> Vec<u64> {
        player
            .and_then(|player| player.masteries)
            .map(|masteries| masteries.into_iter().map(u64::from).collect())
            .unwrap_or_default()
    }
}

impl ReplayAnalysisOps {
    fn cache_player_units(player: Option<&CachePlayer>) -> Value {
        let hidden_units = HashSet::new();
        ReplayAnalysisOps::cache_player_units_with_hidden_units(player, &hidden_units)
    }
}

impl ReplayAnalysisOps {
    fn cache_player_units_with_hidden_units(
        player: Option<&CachePlayer>,
        hidden_units: &HashSet<String>,
    ) -> Value {
        player
            .and_then(|player| player.units.as_ref())
            .map(
                |units: &std::collections::BTreeMap<String, CacheUnitStats>| {
                    ReplayAnalysisOps::sanitize_hidden_unit_stats_with_hidden_units(
                        ReplayAnalysisOps::cache_json_value(units),
                        hidden_units,
                    )
                },
            )
            .unwrap_or_else(|| Value::Object(Default::default()))
    }
}

impl ReplayAnalysisOps {
    fn cache_player_icons(player: Option<&CachePlayer>) -> Value {
        player
            .and_then(|player| player.icons.as_ref())
            .map(
                |icons: &std::collections::BTreeMap<String, CacheIconValue>| {
                    ReplayAnalysisOps::cache_json_value(icons)
                },
            )
            .unwrap_or_else(|| Value::Object(Default::default()))
    }
}

impl ReplayAnalysisOps {
    fn replay_chat_messages_from_cache(messages: &[ReplayMessage]) -> Vec<ReplayChatMessage> {
        messages
            .iter()
            .map(|message| ReplayChatMessage {
                player: message.player,
                text: message.text.clone(),
                time: message.time,
            })
            .collect()
    }
}

impl ReplayAnalysisOps {
    fn replay_chat_messages_from_report(
        messages: &[ParsedReplayMessage],
    ) -> Vec<ReplayChatMessage> {
        messages
            .iter()
            .map(|message| ReplayChatMessage {
                player: message.player,
                text: message.text.clone(),
                time: message.time,
            })
            .collect()
    }
}

impl ReplayAnalysisOps {
    pub fn replay_info_from_cache_entry_with_dictionary(
        entry: &CacheReplayEntry,
        dictionary: &Sc2DictionaryData,
    ) -> ReplayInfo {
        let player_one = ReplayAnalysisOps::cache_player(entry, 1);
        let player_two = ReplayAnalysisOps::cache_player(entry, 2);
        let hidden_units = ReplayAnalysisOps::hidden_unit_stats_names_with_dictionary(dictionary);
        let slot1 = ReplayPlayerInfo {
            name: ReplayAnalysisOps::cache_player_text(player_one, |player| player.name.as_ref()),
            handle: ReplayAnalysisOps::cache_player_text(player_one, |player| {
                player.handle.as_ref()
            }),
            apm: ReplayAnalysisOps::cache_player_u64(player_one, |player| {
                player.apm.map(u64::from)
            }),
            kills: ReplayAnalysisOps::cache_player_u64(player_one, |player| player.kills),
            commander: ReplayAnalysisOps::cache_player_text(player_one, |player| {
                player.commander.as_ref()
            }),
            commander_level: ReplayAnalysisOps::cache_player_u64(player_one, |player| {
                player.commander_level.map(u64::from)
            }),
            mastery_level: ReplayAnalysisOps::cache_player_u64(player_one, |player| {
                player.commander_mastery_level.map(u64::from)
            }),
            prestige: ReplayAnalysisOps::cache_player_u64(player_one, |player| {
                player.prestige.map(u64::from)
            }),
            masteries: ReplayAnalysisOps::cache_player_masteries(player_one),
            units: ReplayAnalysisOps::cache_player_units_with_hidden_units(
                player_one,
                &hidden_units,
            ),
            icons: ReplayAnalysisOps::cache_player_icons(player_one),
        };
        let slot2 = ReplayPlayerInfo {
            name: ReplayAnalysisOps::cache_player_text(player_two, |player| player.name.as_ref()),
            handle: ReplayAnalysisOps::cache_player_text(player_two, |player| {
                player.handle.as_ref()
            }),
            apm: ReplayAnalysisOps::cache_player_u64(player_two, |player| {
                player.apm.map(u64::from)
            }),
            kills: ReplayAnalysisOps::cache_player_u64(player_two, |player| player.kills),
            commander: ReplayAnalysisOps::cache_player_text(player_two, |player| {
                player.commander.as_ref()
            }),
            commander_level: ReplayAnalysisOps::cache_player_u64(player_two, |player| {
                player.commander_level.map(u64::from)
            }),
            mastery_level: ReplayAnalysisOps::cache_player_u64(player_two, |player| {
                player.commander_mastery_level.map(u64::from)
            }),
            prestige: ReplayAnalysisOps::cache_player_u64(player_two, |player| {
                player.prestige.map(u64::from)
            }),
            masteries: ReplayAnalysisOps::cache_player_masteries(player_two),
            units: ReplayAnalysisOps::cache_player_units_with_hidden_units(
                player_two,
                &hidden_units,
            ),
            icons: ReplayAnalysisOps::cache_player_icons(player_two),
        };
        let normalized_mutators = entry
            .mutators
            .iter()
            .map(|mutator| {
                ReplayAnalysisOps::normalize_mutator_id_with_dictionary(mutator, dictionary)
            })
            .collect::<Vec<_>>();
        let weekly_name = if entry.weekly {
            ReplayAnalysisOps::resolve_weekly_mutation_name_with_dictionary(
                &entry.map_name,
                &normalized_mutators,
                dictionary,
            )
        } else {
            None
        };
        let bonus_total = dictionary
            .canonicalize_coop_map_id(&entry.map_name)
            .as_deref()
            .and_then(|map_id| dictionary.coop_map_id_to_english(map_id))
            .as_deref()
            .and_then(|map_name| {
                ReplayAnalysisOps::bonus_objective_total_for_canonical_map_with_dictionary(
                    map_name, dictionary,
                )
            });
        let file_path = Path::new(&entry.file);
        let accurate_length = ReplayAnalysisOps::accurate_length_seconds_from_cache(
            &entry.accurate_length,
            entry.length,
        );
        let difficulty = if !entry.ext_difficulty.trim().is_empty() {
            entry.ext_difficulty.trim().to_string()
        } else if !entry.difficulty.1.trim().is_empty() {
            entry.difficulty.1.trim().to_string()
        } else if !entry.difficulty.0.trim().is_empty() {
            entry.difficulty.0.trim().to_string()
        } else {
            "Unknown".to_string()
        };

        ReplayInfo {
            file: entry.file.clone(),
            date: ReplayAnalysisOps::parse_replay_timestamp_seconds(&entry.date)
                .unwrap_or_else(|| ReplayAnalysisOps::file_modified_seconds(file_path)),
            map: dictionary
                .canonicalize_coop_map_id(&entry.map_name)
                .unwrap_or_else(|| entry.map_name.clone()),
            result: entry.result.clone(),
            difficulty,
            enemy: entry
                .enemy_race
                .as_ref()
                .filter(|value| !value.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| "Unknown".to_string()),
            length: ReplayAnalysisOps::display_length_seconds(accurate_length),
            accurate_length,
            slot1,
            slot2,
            main_slot: 0,
            amon_units: entry
                .amon_units
                .as_ref()
                .map(ReplayAnalysisOps::cache_json_value)
                .unwrap_or_else(|| Value::Object(Default::default())),
            player_stats: entry
                .player_stats
                .as_ref()
                .map(ReplayAnalysisOps::cache_json_value)
                .unwrap_or_else(|| Value::Object(Default::default())),
            extension: entry.extension,
            brutal_plus: u64::from(entry.brutal_plus),
            weekly: entry.weekly,
            weekly_name,
            mutators: normalized_mutators,
            comp: entry
                .comp
                .clone()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "Unidentified AI".to_string()),
            bonus: entry
                .bonus
                .as_ref()
                .map(|bonus| vec![1; bonus.len()])
                .unwrap_or_default(),
            bonus_total,
            messages: ReplayAnalysisOps::replay_chat_messages_from_cache(&entry.messages),
            is_detailed: entry.detailed_analysis,
        }
    }
}

impl ReplayAnalysisOps {
    pub fn replay_info_from_cache_entry(entry: &CacheReplayEntry) -> ReplayInfo {
        let player_one = ReplayAnalysisOps::cache_player(entry, 1);
        let player_two = ReplayAnalysisOps::cache_player(entry, 2);
        let slot1 = ReplayPlayerInfo {
            name: ReplayAnalysisOps::cache_player_text(player_one, |player| player.name.as_ref()),
            handle: ReplayAnalysisOps::cache_player_text(player_one, |player| {
                player.handle.as_ref()
            }),
            apm: ReplayAnalysisOps::cache_player_u64(player_one, |player| {
                player.apm.map(u64::from)
            }),
            kills: ReplayAnalysisOps::cache_player_u64(player_one, |player| player.kills),
            commander: ReplayAnalysisOps::cache_player_text(player_one, |player| {
                player.commander.as_ref()
            }),
            commander_level: ReplayAnalysisOps::cache_player_u64(player_one, |player| {
                player.commander_level.map(u64::from)
            }),
            mastery_level: ReplayAnalysisOps::cache_player_u64(player_one, |player| {
                player.commander_mastery_level.map(u64::from)
            }),
            prestige: ReplayAnalysisOps::cache_player_u64(player_one, |player| {
                player.prestige.map(u64::from)
            }),
            masteries: ReplayAnalysisOps::cache_player_masteries(player_one),
            units: ReplayAnalysisOps::cache_player_units(player_one),
            icons: ReplayAnalysisOps::cache_player_icons(player_one),
        };
        let slot2 = ReplayPlayerInfo {
            name: ReplayAnalysisOps::cache_player_text(player_two, |player| player.name.as_ref()),
            handle: ReplayAnalysisOps::cache_player_text(player_two, |player| {
                player.handle.as_ref()
            }),
            apm: ReplayAnalysisOps::cache_player_u64(player_two, |player| {
                player.apm.map(u64::from)
            }),
            kills: ReplayAnalysisOps::cache_player_u64(player_two, |player| player.kills),
            commander: ReplayAnalysisOps::cache_player_text(player_two, |player| {
                player.commander.as_ref()
            }),
            commander_level: ReplayAnalysisOps::cache_player_u64(player_two, |player| {
                player.commander_level.map(u64::from)
            }),
            mastery_level: ReplayAnalysisOps::cache_player_u64(player_two, |player| {
                player.commander_mastery_level.map(u64::from)
            }),
            prestige: ReplayAnalysisOps::cache_player_u64(player_two, |player| {
                player.prestige.map(u64::from)
            }),
            masteries: ReplayAnalysisOps::cache_player_masteries(player_two),
            units: ReplayAnalysisOps::cache_player_units(player_two),
            icons: ReplayAnalysisOps::cache_player_icons(player_two),
        };
        let file_path = Path::new(&entry.file);
        let accurate_length = ReplayAnalysisOps::accurate_length_seconds_from_cache(
            &entry.accurate_length,
            entry.length,
        );
        let difficulty = if !entry.ext_difficulty.trim().is_empty() {
            entry.ext_difficulty.trim().to_string()
        } else if !entry.difficulty.1.trim().is_empty() {
            entry.difficulty.1.trim().to_string()
        } else if !entry.difficulty.0.trim().is_empty() {
            entry.difficulty.0.trim().to_string()
        } else {
            "Unknown".to_string()
        };

        ReplayInfo {
            file: entry.file.clone(),
            date: ReplayAnalysisOps::parse_replay_timestamp_seconds(&entry.date)
                .unwrap_or_else(|| ReplayAnalysisOps::file_modified_seconds(file_path)),
            map: entry.map_name.clone(),
            result: entry.result.clone(),
            difficulty,
            enemy: entry
                .enemy_race
                .as_ref()
                .filter(|value| !value.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| "Unknown".to_string()),
            length: ReplayAnalysisOps::display_length_seconds(accurate_length),
            accurate_length,
            slot1,
            slot2,
            main_slot: 0,
            amon_units: entry
                .amon_units
                .as_ref()
                .map(ReplayAnalysisOps::cache_json_value)
                .unwrap_or_else(|| Value::Object(Default::default())),
            player_stats: entry
                .player_stats
                .as_ref()
                .map(ReplayAnalysisOps::cache_json_value)
                .unwrap_or_else(|| Value::Object(Default::default())),
            extension: entry.extension,
            brutal_plus: u64::from(entry.brutal_plus),
            weekly: entry.weekly,
            weekly_name: None,
            mutators: entry.mutators.clone(),
            comp: entry
                .comp
                .clone()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "Unidentified AI".to_string()),
            bonus: entry
                .bonus
                .as_ref()
                .map(|bonus| vec![1; bonus.len()])
                .unwrap_or_default(),
            bonus_total: None,
            messages: ReplayAnalysisOps::replay_chat_messages_from_cache(&entry.messages),
            is_detailed: entry.detailed_analysis,
        }
    }
}

impl ReplayAnalysisOps {
    pub(super) fn replay_info_from_report_with_dictionary(
        path: &Path,
        report: &ReplayReport,
        dictionary: &Sc2DictionaryData,
    ) -> ReplayInfo {
        let hidden_units = ReplayAnalysisOps::hidden_unit_stats_names_with_dictionary(dictionary);
        let normalized_mutators = report
            .mutators
            .iter()
            .map(|mutator| {
                ReplayAnalysisOps::normalize_mutator_id_with_dictionary(mutator, dictionary)
            })
            .collect::<Vec<_>>();
        let weekly_name = if report.weekly {
            ReplayAnalysisOps::resolve_weekly_mutation_name_with_dictionary(
                &report.map_name,
                &normalized_mutators,
                dictionary,
            )
        } else {
            None
        };
        let bonus_total = dictionary
            .canonicalize_coop_map_id(&report.map_name)
            .as_deref()
            .and_then(|map_id| dictionary.coop_map_id_to_english(map_id))
            .as_deref()
            .and_then(|map_name| {
                ReplayAnalysisOps::bonus_objective_total_for_canonical_map_with_dictionary(
                    map_name, dictionary,
                )
            });
        let slot1_player = ReplayAnalysisOps::report_player(report, 1);
        let slot2_player = ReplayAnalysisOps::report_player(report, 2);
        let accurate_length =
            if report.parser.accurate_length.is_finite() && report.parser.accurate_length > 0.0 {
                report.parser.accurate_length
            } else {
                report.length.max(0.0)
            };
        let main_slot = match report.positions.main {
            2 => 1,
            _ => 0,
        };
        let slot_player = |slot_index: usize,
                           player: Option<&ParsedReplayPlayer>,
                           commander: &str,
                           commander_level: u64,
                           mastery_level: u64,
                           prestige: u64,
                           masteries: Vec<u64>,
                           units: Value,
                           icons: Value,
                           kills: u64|
         -> ReplayPlayerInfo {
            let fallback_name = if slot_index == 0 {
                report.main.clone()
            } else {
                report.ally.clone()
            };
            ReplayPlayerInfo {
                name: player
                    .map(|value| value.name.clone())
                    .unwrap_or_else(|| fallback_name),
                handle: player.map(|value| value.handle.clone()).unwrap_or_default(),
                apm: player.map(|value| u64::from(value.apm)).unwrap_or(0),
                kills,
                commander: player
                    .map(|value| value.commander.clone())
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| commander.to_string()),
                commander_level: player
                    .map(|value| u64::from(value.commander_level))
                    .unwrap_or(commander_level),
                mastery_level: player
                    .map(|value| u64::from(value.commander_mastery_level))
                    .unwrap_or(mastery_level),
                prestige: player
                    .map(|value| u64::from(value.prestige))
                    .unwrap_or(prestige),
                masteries: player
                    .map(|value| {
                        value
                            .masteries
                            .iter()
                            .map(|entry| u64::from(*entry))
                            .collect()
                    })
                    .unwrap_or(masteries),
                units,
                icons,
            }
        };
        let slot1_is_main = main_slot == 0;
        let slot1 = slot_player(
            0,
            slot1_player,
            if slot1_is_main {
                &report.main_commander
            } else {
                &report.ally_commander
            },
            if slot1_is_main {
                u64::from(report.main_commander_level)
            } else {
                u64::from(report.ally_commander_level)
            },
            slot1_player
                .map(|value| u64::from(value.commander_mastery_level))
                .unwrap_or(0),
            slot1_player
                .map(|value| u64::from(value.prestige))
                .unwrap_or(0),
            if slot1_is_main {
                report
                    .main_masteries
                    .iter()
                    .map(|value| u64::from(*value))
                    .collect()
            } else {
                report
                    .ally_masteries
                    .iter()
                    .map(|value| u64::from(*value))
                    .collect()
            },
            ReplayAnalysisOps::sanitize_hidden_unit_stats_with_hidden_units(
                ReplayAnalysisOps::report_value(if slot1_is_main {
                    &report.main_units
                } else {
                    &report.ally_units
                }),
                &hidden_units,
            ),
            ReplayAnalysisOps::with_outlaw_icons(
                ReplayAnalysisOps::report_value(if slot1_is_main {
                    &report.main_icons
                } else {
                    &report.ally_icons
                }),
                if slot1_is_main {
                    &report.main_commander
                } else {
                    &report.ally_commander
                },
                if (if slot1_is_main {
                    &report.main_commander
                } else {
                    &report.ally_commander
                }) == "Tychus"
                {
                    report.outlaw_order.as_ref()
                } else {
                    None
                },
            ),
            if slot1_is_main {
                report.main_kills
            } else {
                report.ally_kills
            },
        );
        let slot2 = slot_player(
            1,
            slot2_player,
            if slot1_is_main {
                &report.ally_commander
            } else {
                &report.main_commander
            },
            if slot1_is_main {
                u64::from(report.ally_commander_level)
            } else {
                u64::from(report.main_commander_level)
            },
            slot2_player
                .map(|value| u64::from(value.commander_mastery_level))
                .unwrap_or(0),
            slot2_player
                .map(|value| u64::from(value.prestige))
                .unwrap_or(0),
            if slot1_is_main {
                report
                    .ally_masteries
                    .iter()
                    .map(|value| u64::from(*value))
                    .collect()
            } else {
                report
                    .main_masteries
                    .iter()
                    .map(|value| u64::from(*value))
                    .collect()
            },
            ReplayAnalysisOps::sanitize_hidden_unit_stats_with_hidden_units(
                ReplayAnalysisOps::report_value(if slot1_is_main {
                    &report.ally_units
                } else {
                    &report.main_units
                }),
                &hidden_units,
            ),
            ReplayAnalysisOps::with_outlaw_icons(
                ReplayAnalysisOps::report_value(if slot1_is_main {
                    &report.ally_icons
                } else {
                    &report.main_icons
                }),
                if slot1_is_main {
                    &report.ally_commander
                } else {
                    &report.main_commander
                },
                if (if slot1_is_main {
                    &report.ally_commander
                } else {
                    &report.main_commander
                }) == "Tychus"
                {
                    report.outlaw_order.as_ref()
                } else {
                    None
                },
            ),
            if slot1_is_main {
                report.ally_kills
            } else {
                report.main_kills
            },
        );

        ReplayInfo {
            file: path.display().to_string(),
            date: ReplayAnalysisOps::parse_replay_timestamp_seconds(&report.parser.date)
                .unwrap_or_else(|| ReplayAnalysisOps::file_modified_seconds(path)),
            map: dictionary
                .canonicalize_coop_map_id(&report.map_name)
                .unwrap_or_else(|| report.map_name.clone()),
            result: report.result.clone(),
            difficulty: report.difficulty.clone(),
            enemy: if report.parser.enemy_race.trim().is_empty() {
                "Unknown".to_string()
            } else {
                report.parser.enemy_race.clone()
            },
            length: ReplayAnalysisOps::display_length_seconds(accurate_length),
            accurate_length,
            slot1,
            slot2,
            main_slot,
            amon_units: ReplayAnalysisOps::report_value(&report.amon_units),
            player_stats: ReplayAnalysisOps::report_value(&report.player_stats),
            extension: report.extension,
            brutal_plus: u64::from(report.brutal_plus),
            weekly: report.weekly,
            weekly_name,
            mutators: normalized_mutators,
            comp: report.comp.clone(),
            bonus: vec![1; report.bonus.len()],
            bonus_total,
            messages: ReplayAnalysisOps::replay_chat_messages_from_report(&report.parser.messages),
            is_detailed: true,
        }
    }
}

impl ReplayAnalysisOps {
    pub(super) fn unparsed_replay(path: &Path) -> ReplayInfo {
        ReplayInfo {
            file: path.display().to_string(),
            date: ReplayAnalysisOps::file_modified_seconds(path),
            map: "Unknown map".to_string(),
            result: "Unparsed".to_string(),
            difficulty: "Unknown".to_string(),
            enemy: "Unknown".to_string(),
            comp: "Unidentified AI".to_string(),
            accurate_length: 0.0,
            ..ReplayInfo::default()
        }
    }
}
