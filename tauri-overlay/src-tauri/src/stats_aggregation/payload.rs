use serde::Serialize;
use serde_json::{Map, Value};

#[derive(Serialize)]
pub struct StatsAggregateFastestMapDetails {
    length: f64,
    file: String,
    date: u64,
    difficulty: String,
    players: Vec<Value>,
    enemy_race: String,
}

#[derive(Clone, Copy)]
pub struct StatsResultSummary {
    victory: u64,
    defeat: u64,
    winrate: f64,
}

#[derive(Serialize)]
pub struct StatsAggregateMapDataRow {
    id: String,
    average_victory_time: f64,
    frequency: f64,
    #[serde(rename = "Victory")]
    victory: u64,
    #[serde(rename = "Defeat")]
    defeat: u64,
    #[serde(rename = "Winrate")]
    winrate: f64,
    bonus: f64,
    #[serde(rename = "detailedCount")]
    detailed_count: u64,
    #[serde(rename = "Fastest")]
    fastest: StatsAggregateFastestMapDetails,
}

#[derive(Serialize)]
pub struct StatsAggregateDifficultyDataRow {
    #[serde(rename = "Victory")]
    victory: u64,
    #[serde(rename = "Defeat")]
    defeat: u64,
    #[serde(rename = "Winrate")]
    winrate: f64,
}

#[derive(Serialize)]
pub struct StatsAggregateRegionDataRow {
    frequency: f64,
    #[serde(rename = "Victory")]
    victory: u64,
    #[serde(rename = "Defeat")]
    defeat: u64,
    winrate: f64,
    max_asc: u64,
    prestiges: Map<String, Value>,
    max_com: Vec<String>,
}

#[derive(Serialize)]
pub struct StatsAggregatePlayerDataRow {
    wins: u64,
    losses: u64,
    winrate: f64,
    kills: f64,
    apm: f64,
    frequency: f64,
    last_seen: u64,
    commander: String,
}

#[derive(Serialize)]
pub struct StatsAggregateUnitDataPayload {
    main: Value,
    ally: Value,
    amon: Value,
}

#[derive(Serialize)]
pub struct StatsAggregateAnalysisPayload {
    #[serde(rename = "MapData")]
    map_data: Map<String, Value>,
    #[serde(rename = "CommanderData")]
    commander_data: Map<String, Value>,
    #[serde(rename = "AllyCommanderData")]
    ally_commander_data: Map<String, Value>,
    #[serde(rename = "DifficultyData")]
    difficulty_data: Map<String, Value>,
    #[serde(rename = "RegionData")]
    region_data: Map<String, Value>,
    #[serde(rename = "PlayerData")]
    player_data: Map<String, Value>,
    #[serde(rename = "AmonData")]
    amon_data: Map<String, Value>,
    #[serde(rename = "UnitData")]
    unit_data: Value,
    #[serde(rename = "MapDataReady")]
    map_data_ready: bool,
}

impl StatsAggregateFastestMapDetails {
    pub fn new(
        length: f64,
        file: String,
        date: u64,
        difficulty: String,
        players: Vec<Value>,
        enemy_race: String,
    ) -> Self {
        Self {
            length,
            file,
            date,
            difficulty,
            players,
            enemy_race,
        }
    }
}

impl StatsResultSummary {
    pub fn new(victory: u64, defeat: u64, winrate: f64) -> Self {
        Self {
            victory,
            defeat,
            winrate,
        }
    }
}

impl StatsAggregateMapDataRow {
    pub fn new(
        id: String,
        average_victory_time: f64,
        frequency: f64,
        result: StatsResultSummary,
        bonus: f64,
        detailed_count: u64,
        fastest: StatsAggregateFastestMapDetails,
    ) -> Self {
        Self {
            id,
            average_victory_time,
            frequency,
            victory: result.victory,
            defeat: result.defeat,
            winrate: result.winrate,
            bonus,
            detailed_count,
            fastest,
        }
    }
}

impl StatsAggregateDifficultyDataRow {
    pub fn new(result: StatsResultSummary) -> Self {
        Self {
            victory: result.victory,
            defeat: result.defeat,
            winrate: result.winrate,
        }
    }
}

impl StatsAggregateRegionDataRow {
    pub fn new(
        frequency: f64,
        result: StatsResultSummary,
        max_asc: u64,
        prestiges: Map<String, Value>,
        max_com: Vec<String>,
    ) -> Self {
        Self {
            frequency,
            victory: result.victory,
            defeat: result.defeat,
            winrate: result.winrate,
            max_asc,
            prestiges,
            max_com,
        }
    }
}

impl StatsAggregatePlayerDataRow {
    pub fn new(
        result: StatsResultSummary,
        kills: f64,
        apm: f64,
        frequency: f64,
        last_seen: u64,
        commander: String,
    ) -> Self {
        Self {
            wins: result.victory,
            losses: result.defeat,
            winrate: result.winrate,
            kills,
            apm,
            frequency,
            last_seen,
            commander,
        }
    }
}

impl StatsAggregateUnitDataPayload {
    pub fn new(main: Value, ally: Value, amon: Value) -> Self {
        Self { main, ally, amon }
    }
}

impl StatsAggregateAnalysisPayload {
    pub fn new_ready_map_data(
        map_data: Map<String, Value>,
        commander_data: Map<String, Value>,
        ally_commander_data: Map<String, Value>,
        difficulty_data: Map<String, Value>,
        region_data: Map<String, Value>,
        player_data: Map<String, Value>,
        unit_data: Value,
    ) -> Self {
        Self {
            map_data,
            commander_data,
            ally_commander_data,
            difficulty_data,
            region_data,
            player_data,
            amon_data: Map::new(),
            unit_data,
            map_data_ready: true,
        }
    }
}
