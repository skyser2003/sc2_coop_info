use sco_tauri_overlay::StatsAnalysisPayload;
use serde_json::{Value, json};

#[test]
fn stats_analysis_payload_preserves_wire_field_names_and_unit_nulls() {
    let analysis = json!({
        "MapData": {
            "VoidLaunch": {
                "id": "VoidLaunch",
                "average_victory_time": 981.5,
                "frequency": 1.0,
                "Victory": 2,
                "Defeat": 1,
                "Winrate": 0.6666666666666666,
                "bonus": 0.5,
                "detailedCount": 2,
                "Fastest": {
                    "length": 721.0,
                    "file": "fast.SC2Replay",
                    "date": 1_700_000_000,
                    "difficulty": "Brutal",
                    "players": [
                        {
                            "name": "Main",
                            "handle": "1-S2-1-1",
                            "commander": "Raynor",
                            "apm": 123,
                            "mastery_level": 90,
                            "masteries": [30, 0, 30, 0, 30, 0],
                            "prestige": 2,
                            "prestige_name": "Rough Rider"
                        }
                    ],
                    "enemy_race": "Zerg"
                }
            }
        },
        "CommanderData": {
            "Raynor": {
                "Frequency": 1.0,
                "Victory": 2,
                "Defeat": 1,
                "Winrate": 0.6666666666666666,
                "MedianAPM": 120.0,
                "KillFraction": 0.42,
                "Mastery": { "0": 1.0, "1": 0.0 },
                "MasteryDistribution": { "0": { "1": 1.0 } },
                "MasteryDistributionByPrestige": {
                    "2": { "0": { "1": 1.0 } }
                },
                "Prestige": { "2": 1.0 },
                "MasteryByPrestige": { "2": { "0": 1.0 } },
                "detailedCount": 2
            }
        },
        "AllyCommanderData": {},
        "DifficultyData": {
            "Brutal": { "Victory": 2, "Defeat": 1, "Winrate": 0.6666666666666666 }
        },
        "RegionData": {
            "NA": {
                "frequency": 1.0,
                "Victory": 2,
                "Defeat": 1,
                "winrate": 0.6666666666666666,
                "max_asc": 1000,
                "prestiges": { "Raynor": 2 },
                "max_com": ["Raynor"]
            }
        },
        "PlayerData": {
            "Main": {
                "wins": 2,
                "losses": 1,
                "winrate": 0.6666666666666666,
                "kills": 0.42,
                "apm": 120.0,
                "frequency": 1.0,
                "last_seen": 1_700_000_000,
                "commander": "Raynor"
            }
        },
        "AmonData": {
            "Zergling": { "created": 10, "lost": 5, "kills": 1, "KD": "-" }
        },
        "UnitData": {
            "main": {
                "Raynor": {
                    "count": 2,
                    "Marine": {
                        "created": 12,
                        "made": 6.0,
                        "lost": "-",
                        "lost_percent": null,
                        "kills": 30,
                        "KD": null,
                        "kill_percentage": 0.5
                    },
                    "sum": {
                        "created": 12,
                        "made": 1.0,
                        "lost": 0,
                        "lost_percent": 0.0,
                        "kills": 30,
                        "KD": 0.0,
                        "kill_percentage": 1.0
                    }
                },
                "Dehaka": null
            },
            "ally": {},
            "amon": {
                "sum": { "created": 10, "lost": 5, "kills": 1, "KD": 0.2 }
            }
        },
        "MapDataReady": true
    });

    let typed: StatsAnalysisPayload =
        serde_json::from_value(analysis.clone()).expect("stats analysis should deserialize");
    let serialized = serde_json::to_value(typed).expect("stats analysis should serialize");

    assert_eq!(serialized, analysis);
}

#[test]
fn stats_analysis_payload_accepts_empty_payload_without_map_data_ready() {
    let analysis = json!({
        "MapData": {},
        "CommanderData": {},
        "AllyCommanderData": {},
        "DifficultyData": {},
        "RegionData": {},
        "PlayerData": {},
        "AmonData": {},
        "UnitData": Value::Null
    });

    let typed: StatsAnalysisPayload =
        serde_json::from_value(analysis.clone()).expect("empty stats analysis should deserialize");
    let serialized = serde_json::to_value(typed).expect("empty stats analysis should serialize");

    assert_eq!(serialized, analysis);
}
