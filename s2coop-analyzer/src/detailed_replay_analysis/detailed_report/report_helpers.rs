use super::*;

impl DetailedReplayAnalyzer {
    pub(super) fn map_name_has_amon_override(map_name: &str, candidate: &str) -> bool {
        map_name.contains(candidate)
            || (map_name.contains("[MM] Lnl") && candidate == "Lock & Load")
    }

    fn replay_unitid(index: Option<i64>, recycle_index: Option<i64>) -> Option<i64> {
        let index = index?;
        let recycle_index = recycle_index?;
        Some(recycle_index * 100_000 + index)
    }

    pub(super) fn replay_event_unitid(event: &TrackerEvent) -> Option<i64> {
        Self::replay_unitid(event.m_unit_tag_index, event.m_unit_tag_recycle)
    }

    pub(super) fn replay_creator_unitid(event: &TrackerEvent) -> Option<i64> {
        Self::replay_unitid(
            event.m_creator_unit_tag_index,
            event.m_creator_unit_tag_recycle,
        )
    }

    pub(super) fn replay_killer_unitid(event: &TrackerEvent) -> Option<i64> {
        Self::replay_unitid(
            event.m_killer_unit_tag_index,
            event.m_killer_unit_tag_recycle,
        )
    }

    pub(super) fn clamp_nonnegative_to_u64(value: i64) -> u64 {
        if value <= 0 { 0 } else { value as u64 }
    }

    pub(super) fn count_for_pid(values: &[i64], pid: i64) -> i64 {
        usize::try_from(pid)
            .ok()
            .and_then(|index| values.get(index))
            .copied()
            .unwrap_or_default()
    }

    fn round_to_digits_half_even(value: f64, digits: i32) -> f64 {
        if !value.is_finite() {
            return value;
        }
        let Ok(digits_u32) = u32::try_from(digits) else {
            return value;
        };
        let Some(scale10) = 10_u128.checked_pow(digits_u32) else {
            return value;
        };

        let bits = value.to_bits();
        let sign_negative = (bits >> 63) != 0;
        let exponent_bits = ((bits >> 52) & 0x7ff) as i32;
        let mantissa_bits = bits & ((1_u64 << 52) - 1);

        let (mantissa, exponent2) = if exponent_bits == 0 {
            (mantissa_bits as u128, -1074_i32)
        } else {
            (
                (mantissa_bits | (1_u64 << 52)) as u128,
                exponent_bits - 1075,
            )
        };
        if mantissa == 0 {
            return if sign_negative { -0.0 } else { 0.0 };
        }

        let Some(mut numerator) = mantissa.checked_mul(scale10) else {
            return value;
        };
        let mut denominator = 1_u128;
        if exponent2 >= 0 {
            let Ok(shift) = u32::try_from(exponent2) else {
                return value;
            };
            let Some(shifted) = numerator.checked_shl(shift) else {
                return value;
            };
            numerator = shifted;
        } else {
            let Ok(shift) = u32::try_from(-exponent2) else {
                return value;
            };
            let Some(shifted) = denominator.checked_shl(shift) else {
                return 0.0;
            };
            denominator = shifted;
        }

        let quotient = numerator / denominator;
        let remainder = numerator % denominator;
        let rounded = match remainder.checked_mul(2) {
            Some(double_remainder) if double_remainder < denominator => quotient,
            Some(double_remainder) if double_remainder > denominator => quotient + 1,
            Some(_) => {
                if quotient.is_multiple_of(2) {
                    quotient
                } else {
                    quotient + 1
                }
            }
            None => quotient,
        };

        let factor = 10_f64.powi(digits);
        if !factor.is_finite() || factor == 0.0 {
            return value;
        }

        let rounded_value = rounded as f64 / factor;
        if sign_negative {
            -rounded_value
        } else {
            rounded_value
        }
    }

    pub(super) fn format_mm_ss(seconds: f64) -> String {
        if !seconds.is_finite() || seconds <= 0.0 {
            return "00:00".to_string();
        }
        let total = seconds as u64;
        let minutes = (total / 60) % 60;
        let secs = total % 60;
        format!("{minutes:02}:{secs:02}")
    }

    fn contains_skip_strings_text(unit_name: &str, skip_tokens: &[String]) -> bool {
        let lowered = unit_name.to_lowercase();
        skip_tokens.iter().any(|token| lowered.contains(token))
    }

    pub(super) fn increment_icon_count(icons: &mut BTreeMap<String, u64>, key: &str, delta: i64) {
        if delta == 0 {
            return;
        }

        let current = icons.get(key).copied().unwrap_or_default() as i64;
        let next = current + delta;
        if next <= 0 {
            icons.remove(key);
        } else {
            icons.insert(key.to_string(), next as u64);
        }
    }

    pub(super) fn set_icon_count(icons: &mut BTreeMap<String, u64>, key: &str, value: i64) {
        if value > 0 {
            icons.insert(key.to_string(), value as u64);
        } else {
            icons.remove(key);
        }
    }
    fn switched_unit_counts(
        counts: &UnitTypeCountMap,
        unit_name_dict: &UnitNamesJson,
        unit_add_kills_to: &UnitAddKillsToJson,
        unit_add_losses_to: &HashMap<String, String>,
        dont_include_units: &HashSet<String>,
    ) -> HashMap<String, [i64; 4]> {
        let mut switched: HashMap<String, [i64; 4]> = HashMap::new();

        for (unit_name, values) in counts {
            if dont_include_units.contains(unit_name) {
                continue;
            }

            let mut added = false;
            if let Some(target) = unit_add_kills_to.get(unit_name) {
                let entry = switched.entry(target.clone()).or_insert([0_i64; 4]);
                entry[2] += values[2];
                added = true;
            }

            if let Some(target) = unit_add_losses_to.get(unit_name) {
                let entry = switched.entry(target.clone()).or_insert([0_i64; 4]);
                entry[1] += values[1];
                added = true;
            }

            if !added {
                let mapped_name = unit_name_dict
                    .get(unit_name)
                    .cloned()
                    .unwrap_or_else(|| unit_name.clone());
                let entry = switched.entry(mapped_name).or_insert([0_i64; 4]);
                for (index, value) in values.iter().enumerate() {
                    entry[index] += *value;
                }
            }
        }

        switched
    }

    fn sorted_switch_name_entries(
        counts: &UnitTypeCountMap,
        unit_name_dict: &UnitNamesJson,
        unit_add_kills_to: &UnitAddKillsToJson,
        unit_add_losses_to: &HashMap<String, String>,
        dont_include_units: &HashSet<String>,
    ) -> Vec<(String, i64, i64, i64)> {
        let mut rows = DetailedReplayAnalyzer::switched_unit_counts(
            counts,
            unit_name_dict,
            unit_add_kills_to,
            unit_add_losses_to,
            dont_include_units,
        )
        .into_iter()
        .map(|(unit_name, values)| (unit_name, values[0], values[1], values[2]))
        .collect::<Vec<(String, i64, i64, i64)>>();

        rows.sort_by(|left, right| {
            right
                .3
                .cmp(&left.3)
                .then_with(|| right.1.cmp(&left.1))
                .then_with(|| left.0.cmp(&right.0))
        });
        rows
    }

    fn unit_stats_tuple(created: i64, lost: i64, kills: i64, kill_fraction: f64) -> UnitStats {
        (created, lost, kills, kill_fraction)
    }

    pub(super) fn fill_unit_kills_and_icons(
        input: FillUnitKillsAndIconsInput<'_>,
    ) -> (BTreeMap<String, UnitStats>, BTreeMap<String, u64>) {
        let FillUnitKillsAndIconsInput {
            base_icons,
            player,
            main_player,
            unit_counts,
            ally_kills_counted_toward_main,
            killcounts,
            unit_name_dict,
            unit_add_kills_to,
            unit_add_losses_to,
            analysis_sets,
        } = input;
        let mut icons = base_icons.clone();
        for (unit_name, values) in unit_counts {
            let created = values[0];
            if analysis_sets
                .locust_source_units
                .contains(unit_name.as_str())
            {
                DetailedReplayAnalyzer::increment_icon_count(&mut icons, "locust", created);
            } else if analysis_sets
                .broodling_source_units
                .contains(unit_name.as_str())
            {
                DetailedReplayAnalyzer::increment_icon_count(&mut icons, "broodling", created);
            }
        }

        for icon_key in ["broodling", "locust"] {
            let count = icons.get(icon_key).copied().unwrap_or_default();
            if count > 0 && count < 200 {
                icons.remove(icon_key);
            }
        }

        let rows = DetailedReplayAnalyzer::sorted_switch_name_entries(
            unit_counts,
            unit_name_dict,
            unit_add_kills_to,
            unit_add_losses_to,
            &analysis_sets.dont_include_units,
        );
        let player_kills = DetailedReplayAnalyzer::count_for_pid(killcounts, player);
        let dehaka_created_lost = rows
            .iter()
            .find(|(unit_name, _, _, _)| unit_name == "Dehaka")
            .map(|(_, created, lost, _)| (*created, *lost));

        let mut units = BTreeMap::new();
        for (unit_name, mut created, mut lost, kills) in rows {
            let denominator = if ally_kills_counted_toward_main > 0 && player != main_player {
                player_kills + ally_kills_counted_toward_main
            } else if ally_kills_counted_toward_main > 0 && player == main_player {
                player_kills - ally_kills_counted_toward_main
            } else {
                player_kills
            };

            let kill_fraction = if denominator > 0 {
                DetailedReplayAnalyzer::round_to_digits_half_even(
                    kills as f64 / denominator as f64,
                    2,
                )
            } else {
                0.0
            };

            if unit_name == "Zweihaka"
                && let Some((dehaka_created, dehaka_lost)) = dehaka_created_lost
            {
                created = dehaka_created;
                lost = dehaka_lost;
            }

            units.insert(
                unit_name.clone(),
                DetailedReplayAnalyzer::unit_stats_tuple(created, lost, kills, kill_fraction),
            );

            if analysis_sets.icon_units.contains(&unit_name) {
                DetailedReplayAnalyzer::set_icon_count(&mut icons, &unit_name, created);
            }
        }

        let mut artifacts_collected = 0_i64;
        for (unit_name, values) in unit_counts {
            let created = values[0];
            let lost = values[1];

            if analysis_sets
                .zeratul_artifact_pickups
                .contains(unit_name.as_str())
            {
                artifacts_collected += lost;
            }
            if analysis_sets
                .zeratul_shade_projections
                .contains(unit_name.as_str())
            {
                DetailedReplayAnalyzer::increment_icon_count(
                    &mut icons,
                    "ShadeProjection",
                    created,
                );
            }
        }
        if artifacts_collected > 0 {
            DetailedReplayAnalyzer::set_icon_count(&mut icons, "Artifact", artifacts_collected);
        }

        (units, icons)
    }

    pub(super) fn fill_amon_units(
        unit_counts: &UnitTypeCountMap,
        killcounts: &[i64],
        amon_players: &ReplayPlayerIdSet,
        unit_name_dict: &UnitNamesJson,
        unit_add_kills_to: &UnitAddKillsToJson,
        unit_add_losses_to: &HashMap<String, String>,
        analysis_sets: &ReplayAnalysisSets,
    ) -> BTreeMap<String, UnitStats> {
        let rows = DetailedReplayAnalyzer::sorted_switch_name_entries(
            unit_counts,
            unit_name_dict,
            unit_add_kills_to,
            unit_add_losses_to,
            &analysis_sets.dont_include_units,
        );

        let mut total_amon_kills = amon_players
            .iter()
            .map(|player| DetailedReplayAnalyzer::count_for_pid(killcounts, player))
            .sum::<i64>();
        if total_amon_kills == 0 {
            total_amon_kills = 1;
        }

        let mut amon_units = BTreeMap::new();
        for (unit_name, created, lost, kills) in rows {
            if DetailedReplayAnalyzer::contains_skip_strings_text(
                &unit_name,
                &analysis_sets.skip_tokens,
            ) {
                continue;
            }
            let kill_fraction = DetailedReplayAnalyzer::round_to_digits_half_even(
                kills as f64 / total_amon_kills as f64,
                2,
            );
            amon_units.insert(
                unit_name,
                DetailedReplayAnalyzer::unit_stats_tuple(created, lost, kills, kill_fraction),
            );
        }
        amon_units
    }

    pub(super) fn enemy_comp_from_identified_waves(
        identified_waves: &IdentifiedWavesMap,
        unit_comp_dict: &HashMap<String, Vec<HashSet<String>>>,
    ) -> String {
        let mut ai_order = unit_comp_dict.keys().collect::<Vec<&String>>();
        ai_order.sort();
        let mut scores = ai_order
            .iter()
            .map(|ai| (*ai, 0.0_f64))
            .collect::<Vec<(&String, f64)>>();

        for wave in identified_waves.values() {
            let types = wave.iter().map(String::as_str).collect::<HashSet<&str>>();
            if types.is_empty() {
                continue;
            }

            for (ai, score) in &mut scores {
                let Some(waves) = unit_comp_dict.get(ai.as_str()) else {
                    continue;
                };
                for wave_row in waves {
                    let wave_len = if wave_row.contains("Medivac") {
                        wave_row.len().saturating_sub(1)
                    } else {
                        wave_row.len()
                    };
                    let types_match_wave = types
                        .iter()
                        .all(|unit_type| *unit_type != "Medivac" && wave_row.contains(*unit_type));
                    if types_match_wave && types.len() == wave_len {
                        *score += wave_len as f64;
                    } else if types_match_wave && wave_len.saturating_sub(types.len()) == 1 {
                        *score += 0.25 * wave_len as f64;
                    }
                }
            }
        }

        let mut best_ai: Option<&String> = None;
        let mut best_score = 0.0_f64;
        for (ai, score) in scores {
            if score > best_score {
                best_score = score;
                best_ai = Some(ai);
            }
        }

        best_ai
            .cloned()
            .unwrap_or_else(|| "Unidentified AI".to_string())
    }

    pub(super) fn apply_custom_kill_icons(
        main_icons: &mut BTreeMap<String, u64>,
        ally_icons: &mut BTreeMap<String, u64>,
        custom_kill_count: &replay_event_handlers::NestedPlayerCountMap,
        unit_type_dict_amon: &UnitTypeCountMap,
        map_flags: &ReplayMapAnalysisFlags,
        main_player: i64,
        ally_player: i64,
    ) {
        for key in CUSTOM_KILL_ICON_KEYS {
            let Some(player_counts) = custom_kill_count.get(key) else {
                continue;
            };
            if key == "deadofnight" && !map_flags.is_dead_of_night() {
                continue;
            }
            if key == "minesweeper" && !unit_type_dict_amon.contains_key("MutatorSpiderMine") {
                continue;
            }

            main_icons.insert(
                key.to_string(),
                player_counts
                    .get(&main_player)
                    .copied()
                    .unwrap_or_default()
                    .max(0) as u64,
            );
            ally_icons.insert(
                key.to_string(),
                player_counts
                    .get(&ally_player)
                    .copied()
                    .unwrap_or_default()
                    .max(0) as u64,
            );
        }
    }
}
