use super::*;

impl ReplayVisualReplayInfo {
    pub fn new(
        file: impl Into<String>,
        map: impl Into<String>,
        result: impl Into<String>,
        duration_seconds: f64,
    ) -> Self {
        Self {
            file: file.into(),
            map: map.into(),
            result: result.into(),
            duration_seconds,
        }
    }
}

impl ReplayVisualMapSize {
    pub fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }
}

impl ReplayVisualContext {
    pub fn new(
        file: impl Into<String>,
        map: impl Into<String>,
        result: impl Into<String>,
        duration_seconds: u64,
        main_player_id: i64,
    ) -> Self {
        Self {
            file: file.into(),
            map: map.into(),
            result: result.into(),
            duration_seconds,
            main_player_id,
        }
    }

    pub(super) fn file(&self) -> &str {
        self.file.as_str()
    }

    pub(super) fn map(&self) -> &str {
        self.map.as_str()
    }

    pub(super) fn result(&self) -> &str {
        self.result.as_str()
    }

    pub(super) fn duration_seconds(&self) -> u64 {
        self.duration_seconds
    }

    pub(super) fn main_player_id(&self) -> i64 {
        self.main_player_id
    }
}

impl ReplayVisualDictionaries {
    pub fn new(
        unit_names: HashMap<String, String>,
        units_in_waves: HashSet<String>,
        amon_player_ids: HashSet<i64>,
    ) -> Self {
        Self::new_with_omitted_units(unit_names, units_in_waves, amon_player_ids, HashSet::new())
    }

    pub fn new_with_omitted_units(
        unit_names: HashMap<String, String>,
        units_in_waves: HashSet<String>,
        amon_player_ids: HashSet<i64>,
        omitted_unit_types: HashSet<String>,
    ) -> Self {
        let mut omitted_unit_types = omitted_unit_types;
        omitted_unit_types.extend(Self::visualizer_omitted_unit_types());
        Self {
            unit_names,
            units_in_waves,
            amon_player_ids,
            omitted_unit_types,
            omitted_unit_type_prefixes: Self::visualizer_omitted_unit_type_prefixes(),
            omitted_unit_type_or_name_fragments:
                Self::visualizer_omitted_unit_type_or_name_fragments(),
        }
    }

    fn visualizer_omitted_unit_types() -> HashSet<String> {
        HashSet::from(["CreepTumorStukov".to_string()])
    }

    fn visualizer_omitted_unit_type_prefixes() -> Vec<String> {
        vec![
            "AbathurSymbiote".to_string(),
            "Beacon".to_string(),
            "CoopCaster".to_string(),
            "SOACaster".to_string(),
        ]
    }

    fn visualizer_omitted_unit_type_or_name_fragments() -> Vec<String> {
        vec![
            "cocoon".to_string(),
            "dummy".to_string(),
            "egg".to_string(),
            "larva".to_string(),
            "mineralfield".to_string(),
            "pathingblocker".to_string(),
            "pickup".to_string(),
            "placeholder".to_string(),
            "top bar".to_string(),
            "unbuildable".to_string(),
            "vespenegeyser".to_string(),
        ]
    }

    pub(super) fn from_dictionary(dictionary: &Sc2DictionaryData, map_name: &str) -> Self {
        let unit_names = Self::clone_unit_names(&dictionary.unit_name_dict);
        let units_in_waves = dictionary.units_in_waves.clone();
        let omitted_unit_types = dictionary
            .replay_analysis_data
            .dont_include_units
            .iter()
            .cloned()
            .collect();
        let mut amon_player_ids = HashSet::from([3_i64, 4_i64]);
        for (mission_name, player_ids) in dictionary.amon_player_ids.iter() {
            if !ReplayVisualOps::map_name_has_amon_override(map_name, mission_name) {
                continue;
            }
            amon_player_ids.extend(player_ids.iter().copied());
            break;
        }

        Self::new_with_omitted_units(
            unit_names,
            units_in_waves,
            amon_player_ids,
            omitted_unit_types,
        )
    }

    fn clone_unit_names(unit_names: &UnitNamesJson) -> HashMap<String, String> {
        unit_names
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    pub(super) fn display_name(&self, unit_type: &str) -> String {
        self.unit_names
            .get(unit_type)
            .cloned()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| unit_type.to_string())
    }

    pub(super) fn is_amon_player(&self, player_id: i64) -> bool {
        self.amon_player_ids.contains(&player_id)
    }

    pub(super) fn is_wave_unit(&self, unit_type: &str) -> bool {
        self.units_in_waves.contains(unit_type)
    }

    pub(super) fn should_omit_unit(&self, unit_type: &str, display_name: &str) -> bool {
        if self.omitted_unit_types.contains(unit_type) {
            return true;
        }
        if self
            .omitted_unit_type_prefixes
            .iter()
            .any(|prefix| unit_type.starts_with(prefix))
        {
            return true;
        }
        let lower_type = unit_type.to_ascii_lowercase();
        let lower_name = display_name.to_ascii_lowercase();
        self.omitted_unit_type_or_name_fragments
            .iter()
            .any(|fragment| lower_type.contains(fragment) || lower_name.contains(fragment))
    }
}

impl ReplayVisualBuildInput {
    pub fn new(
        replay: ReplayVisualReplayInfo,
        map_size: ReplayVisualMapSize,
        players: Vec<ReplayVisualPlayer>,
        main_player_id: i64,
    ) -> Self {
        Self {
            file: replay.file,
            map: replay.map,
            result: replay.result,
            duration_seconds: replay.duration_seconds,
            map_width: map_size.width,
            map_height: map_size.height,
            players,
            main_player_id,
        }
    }
}

impl ReplayVisualPlayer {
    pub(super) fn new(
        player_id: i64,
        label: impl Into<String>,
        owner_kind: ReplayVisualOwnerKind,
    ) -> Self {
        Self {
            player_id,
            label: label.into(),
            owner_kind,
            color: ReplayVisualOps::owner_color(owner_kind).to_string(),
        }
    }
}
