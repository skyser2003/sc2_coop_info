use crate::cache_overall_stats_generator::{AnalysisPlayerStatsSeries, ReplayBuildInfo};
use indexmap::IndexMap;
use s2protocol_port::MessageEvent;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

type UnitStats = (i64, i64, i64, f64);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParsedReplayPlayer {
    pub pid: u8,
    pub name: String,
    pub handle: String,
    pub race: String,
    pub observer: bool,
    pub result: String,
    pub commander: String,
    pub commander_level: u32,
    pub commander_mastery_level: u32,
    pub prestige: u32,
    pub prestige_name: String,
    pub apm: u32,
    pub masteries: [u32; 6],
}

impl ParsedReplayPlayer {
    pub(crate) fn empty(pid: u8) -> Self {
        Self {
            pid,
            name: String::new(),
            handle: String::new(),
            race: String::new(),
            observer: false,
            result: String::new(),
            commander: String::new(),
            commander_level: 0,
            commander_mastery_level: 0,
            prestige: 0,
            prestige_name: String::new(),
            apm: 0,
            masteries: [0, 0, 0, 0, 0, 0],
        }
    }

    pub(crate) fn is_placeholder(&self) -> bool {
        self.pid != 0
            && self.name.is_empty()
            && self.handle.is_empty()
            && self.race.is_empty()
            && !self.observer
            && self.result.is_empty()
            && self.commander.is_empty()
            && self.commander_level == 0
            && self.commander_mastery_level == 0
            && self.prestige == 0
            && self.prestige_name.is_empty()
            && self.apm == 0
            && self.masteries.iter().all(|value| *value == 0)
    }

    fn unknown(pid: u8) -> Self {
        Self {
            name: "Unknown".to_string(),
            commander: "Unknown".to_string(),
            ..Self::empty(pid)
        }
    }

    pub(crate) fn normalize_slots(
        players: &[Self],
        prepend_empty_pid0: bool,
        player_limit: Option<usize>,
    ) -> Vec<Self> {
        let mut normalized =
            Vec::with_capacity(players.len() + usize::from(prepend_empty_pid0) + 1);
        if prepend_empty_pid0 {
            normalized.push(Self::empty(0));
        }
        match player_limit {
            Some(limit) => normalized.extend(players.iter().take(limit).cloned()),
            None => normalized.extend(players.iter().cloned()),
        }

        if normalized.len() <= 2 {
            while normalized.len() < 2 {
                normalized.push(Self::empty(normalized.len() as u8));
            }
            normalized.push(Self::empty(2));
        } else if normalized[2].pid != 2 {
            normalized.insert(2, Self::empty(2));
        }

        normalized
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParsedReplayMessage {
    pub text: String,
    pub player: u8,
    pub time: f64,
}

impl ParsedReplayMessage {
    pub(crate) fn from_message_event(message: &MessageEvent) -> Option<Self> {
        let text = if let Some(value) = message.m_string.as_ref().filter(|value| !value.is_empty())
        {
            value.clone()
        } else if message.event == "NNet.Game.SPingMessage" {
            "*pings*".to_string()
        } else {
            return None;
        };
        let player = message.user_id.map(|value| value + 1).unwrap_or_default() as u8;
        let time = message.game_loop as f64 / 16.0;
        Some(Self { text, player, time })
    }

    pub(crate) fn sorted_with_leave_events(
        messages: &[Self],
        user_leave_times: &IndexMap<i64, f64>,
    ) -> Vec<Self> {
        let mut rows = messages
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, message)| (message.time, index, message))
            .collect::<Vec<(f64, usize, Self)>>();

        let base_index = rows.len();
        for (offset, (player, leave_time)) in user_leave_times.iter().enumerate() {
            if *player != 1 && *player != 2 {
                continue;
            }
            rows.push((
                *leave_time,
                base_index + offset,
                Self {
                    player: *player as u8,
                    text: "*has left the game*".to_string(),
                    time: *leave_time,
                },
            ));
        }

        rows.sort_by(|left, right| {
            left.0
                .total_cmp(&right.0)
                .then_with(|| left.1.cmp(&right.1))
        });
        rows.into_iter().map(|(_, _, message)| message).collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParsedReplayInput {
    pub file: String,
    pub map_name: String,
    pub extension: bool,
    pub brutal_plus: u32,
    pub result: String,
    pub players: Vec<ParsedReplayPlayer>,
    pub difficulty: (String, String),
    pub accurate_length: f64,
    pub form_alength: String,
    pub length: u64,
    pub mutators: Vec<String>,
    pub weekly: bool,
    pub messages: Vec<ParsedReplayMessage>,
    pub hash: Option<String>,
    pub build: ReplayBuildInfo,
    pub date: String,
    pub enemy_race: String,
    pub ext_difficulty: String,
    pub region: String,
}

impl ParsedReplayInput {
    pub fn player(&self, pid: u8) -> Option<&ParsedReplayPlayer> {
        self.players.iter().find(|player| player.pid == pid)
    }

    pub(crate) fn player_mut(&mut self, pid: u8) -> Option<&mut ParsedReplayPlayer> {
        self.players.iter_mut().find(|player| player.pid == pid)
    }

    pub(crate) fn player_or_unknown(&self, pid: u8) -> ParsedReplayPlayer {
        self.player(pid)
            .cloned()
            .unwrap_or_else(|| ParsedReplayPlayer::unknown(pid))
    }

    pub(crate) fn selected_main_player_pid(&self, main_player_handles: &HashSet<String>) -> u8 {
        if main_player_handles.is_empty() {
            return 1;
        }

        self.players
            .iter()
            .filter(|player| player.pid == 1 || player.pid == 2)
            .find(|player| main_player_handles.contains(player.handle.as_str()))
            .map(|player| player.pid)
            .unwrap_or(1)
    }

    pub fn apply_player_overrides(
        &mut self,
        commander_by_player: &HashMap<i64, String>,
        mastery_by_player: &HashMap<i64, [i64; 6]>,
        prestige_by_player: &HashMap<i64, String>,
    ) {
        let is_mm = self.file.contains("[MM]");

        for pid in [1_i64, 2_i64] {
            let Some(player) = self.player_mut(pid as u8) else {
                continue;
            };

            if player.commander.trim().is_empty() {
                if let Some(commander_name) = commander_by_player
                    .get(&pid)
                    .filter(|value| !value.trim().is_empty())
                {
                    player.commander = commander_name.clone();
                }
            }

            if let Some(prestige_name) = prestige_by_player
                .get(&pid)
                .filter(|value| !value.trim().is_empty())
            {
                player.prestige_name = prestige_name.clone();
            }

            if is_mm {
                if let Some(masteries) = mastery_by_player.get(&pid) {
                    let mut parsed_masteries = [0_u32; 6];
                    for (index, mastery_value) in masteries.iter().enumerate() {
                        parsed_masteries[index] =
                            u32::try_from((*mastery_value).max(0)).unwrap_or_default();
                    }
                    player.masteries = parsed_masteries;
                }
                player.commander_level = 15;
            }
        }
    }

    pub(crate) fn normalized_cache_players(&self) -> Vec<ParsedReplayPlayer> {
        ParsedReplayPlayer::normalize_slots(&self.players, false, None)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlayerPositions {
    pub main: u8,
    pub ally: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplayReport {
    pub file: String,
    pub replaydata: bool,
    pub map_name: String,
    pub extension: bool,
    #[serde(rename = "B+")]
    pub brutal_plus: u32,
    pub result: String,
    pub main: String,
    pub ally: String,
    #[serde(rename = "mainAPM")]
    pub main_apm: u32,
    #[serde(rename = "allyAPM")]
    pub ally_apm: u32,
    pub positions: PlayerPositions,
    pub difficulty: String,
    #[serde(rename = "mainIcons")]
    pub main_icons: BTreeMap<String, u64>,
    #[serde(rename = "allyIcons")]
    pub ally_icons: BTreeMap<String, u64>,
    pub player_stats: BTreeMap<u8, AnalysisPlayerStatsSeries>,
    pub bonus: Vec<String>,
    pub comp: String,
    pub length: f64,
    pub parser: ParsedReplayInput,
    pub mutators: Vec<String>,
    pub weekly: bool,
    #[serde(rename = "mainCommander")]
    pub main_commander: String,
    #[serde(rename = "mainCommanderLevel")]
    pub main_commander_level: u32,
    #[serde(rename = "mainMasteries")]
    pub main_masteries: [u32; 6],
    #[serde(rename = "mainkills")]
    pub main_kills: u64,
    #[serde(rename = "mainPrestige")]
    pub main_prestige: String,
    #[serde(rename = "allyCommander")]
    pub ally_commander: String,
    #[serde(rename = "allyCommanderLevel")]
    pub ally_commander_level: u32,
    #[serde(rename = "allyMasteries")]
    pub ally_masteries: [u32; 6],
    #[serde(rename = "allykills")]
    pub ally_kills: u64,
    #[serde(rename = "allyPrestige")]
    pub ally_prestige: String,
    #[serde(rename = "mainUnits")]
    pub main_units: BTreeMap<String, UnitStats>,
    #[serde(rename = "allyUnits")]
    pub ally_units: BTreeMap<String, UnitStats>,
    pub amon_units: BTreeMap<String, UnitStats>,
    #[serde(skip)]
    pub outlaw_order: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplayReportDetailedInput {
    pub parser: ParsedReplayInput,
    pub positions: Option<PlayerPositions>,
    pub main_position: Option<u8>,
    pub detail: Option<ReplayReportDetailData>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ReplayReportDetailData {
    pub length: f64,
    pub bonus: Vec<String>,
    pub comp: String,
    pub replay_hash: Option<String>,
    pub main_kills: u64,
    pub ally_kills: u64,
    pub main_icons: BTreeMap<String, u64>,
    pub ally_icons: BTreeMap<String, u64>,
    pub main_units: BTreeMap<String, UnitStats>,
    pub ally_units: BTreeMap<String, UnitStats>,
    pub amon_units: BTreeMap<String, UnitStats>,
    pub player_stats: BTreeMap<u8, AnalysisPlayerStatsSeries>,
    #[serde(skip)]
    pub outlaw_order: Vec<String>,
}

impl ReplayReportDetailedInput {
    pub fn from_parser(parser: ParsedReplayInput) -> Self {
        Self {
            parser,
            positions: None,
            main_position: None,
            detail: None,
        }
    }

    pub(crate) fn selected_main_player_pid(&self, main_player_handles: &HashSet<String>) -> u8 {
        if let Some(positions) = self.positions.as_ref() {
            if matches!(positions.main, 1 | 2) {
                return positions.main;
            }
        }

        if let Some(main_position) = self.main_position {
            if matches!(main_position, 1 | 2) {
                return main_position;
            }
        }

        self.parser.selected_main_player_pid(main_player_handles)
    }
}

impl ReplayReport {
    fn normalized_commander_name(raw: &str) -> String {
        if raw.trim().is_empty() {
            "Unknown".to_string()
        } else {
            raw.to_string()
        }
    }

    fn player_stats_with_names(
        incoming: Option<BTreeMap<u8, AnalysisPlayerStatsSeries>>,
        main_name: &str,
        ally_name: &str,
    ) -> BTreeMap<u8, AnalysisPlayerStatsSeries> {
        let mut player_stats = incoming.unwrap_or_default();
        player_stats
            .entry(1)
            .or_insert_with(|| AnalysisPlayerStatsSeries::empty_named(main_name.to_string()))
            .name = main_name.to_string();
        player_stats
            .entry(2)
            .or_insert_with(|| AnalysisPlayerStatsSeries::empty_named(ally_name.to_string()))
            .name = ally_name.to_string();
        player_stats
    }

    pub fn from_detailed_input(
        replay_file: &str,
        detailed_input: &ReplayReportDetailedInput,
        main_player_handles: &HashSet<String>,
    ) -> Self {
        let replay = &detailed_input.parser;
        let detail = detailed_input.detail.as_ref();
        let main_pid = detailed_input.selected_main_player_pid(main_player_handles);
        let ally_pid = if main_pid == 1 { 2 } else { 1 };
        let main_player = replay.player_or_unknown(main_pid);
        let ally_player = replay.player_or_unknown(ally_pid);
        let player_stats = Self::player_stats_with_names(
            detail.map(|value| value.player_stats.clone()),
            &main_player.name,
            &ally_player.name,
        );

        let report_length = detail
            .map(|value| value.length)
            .unwrap_or(replay.accurate_length);
        let parser_accurate_length =
            if replay.accurate_length.is_finite() && replay.accurate_length > 0.0 {
                replay.accurate_length
            } else {
                report_length
            };
        let parser_hash = replay
            .hash
            .clone()
            .or_else(|| detail.and_then(|value| value.replay_hash.clone()));
        let mut parser = replay.clone();
        parser.accurate_length = parser_accurate_length;
        parser.hash = parser_hash;

        Self {
            file: replay_file.to_string(),
            replaydata: true,
            map_name: replay.map_name.clone(),
            extension: replay.extension,
            brutal_plus: replay.brutal_plus,
            result: replay.result.clone(),
            main: main_player.name.clone(),
            ally: ally_player.name.clone(),
            main_apm: main_player.apm,
            ally_apm: ally_player.apm,
            positions: PlayerPositions {
                main: main_pid,
                ally: ally_pid,
            },
            difficulty: replay.difficulty.1.clone(),
            main_icons: detail
                .map(|value| value.main_icons.clone())
                .unwrap_or_default(),
            ally_icons: detail
                .map(|value| value.ally_icons.clone())
                .unwrap_or_default(),
            player_stats,
            bonus: detail.map(|value| value.bonus.clone()).unwrap_or_default(),
            comp: detail.map(|value| value.comp.clone()).unwrap_or_default(),
            length: report_length,
            parser,
            mutators: replay.mutators.clone(),
            weekly: replay.weekly,
            main_commander: Self::normalized_commander_name(main_player.commander.as_str()),
            main_commander_level: main_player.commander_level,
            main_masteries: main_player.masteries,
            main_kills: detail.map(|value| value.main_kills).unwrap_or(0),
            main_prestige: main_player.prestige_name,
            ally_commander: Self::normalized_commander_name(ally_player.commander.as_str()),
            ally_commander_level: ally_player.commander_level,
            ally_masteries: ally_player.masteries,
            ally_kills: detail.map(|value| value.ally_kills).unwrap_or(0),
            ally_prestige: ally_player.prestige_name,
            main_units: detail
                .map(|value| value.main_units.clone())
                .unwrap_or_default(),
            ally_units: detail
                .map(|value| value.ally_units.clone())
                .unwrap_or_default(),
            amon_units: detail
                .map(|value| value.amon_units.clone())
                .unwrap_or_default(),
            outlaw_order: detail
                .map(|value| value.outlaw_order.clone())
                .filter(|value| !value.is_empty()),
        }
    }

    pub fn from_parser(
        replay_file: &str,
        replay: &ParsedReplayInput,
        main_player_handles: &HashSet<String>,
    ) -> Self {
        let input = ReplayReportDetailedInput::from_parser(replay.clone());
        Self::from_detailed_input(replay_file, &input, main_player_handles)
    }

    pub(crate) fn has_non_empty_player_stats(&self) -> bool {
        [1_u8, 2_u8].into_iter().any(|player_id| {
            self.player_stats.get(&player_id).is_some_and(|stats| {
                !stats.supply.is_empty()
                    || !stats.mining.is_empty()
                    || !stats.army.is_empty()
                    || !stats.killed.is_empty()
            })
        })
    }
}
