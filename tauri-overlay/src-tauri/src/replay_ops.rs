use s2coop_analyzer::dictionary_data::Sc2DictionaryData;
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::path::Path;

use crate::{ReplayAnalysis, ReplayInfo, TauriOverlayOps};

impl TauriOverlayOps {
    pub fn decode_html_entities(value: &str) -> String {
        value
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&apos;", "'")
    }

    pub fn canonical_mutator_id_with_dictionary(
        mutator: &str,
        dictionary: &Sc2DictionaryData,
    ) -> String {
        if dictionary.mutator_data(mutator).is_some() {
            mutator.to_string()
        } else if let Some(mutator_id) = dictionary.mutator_id_from_name(mutator) {
            mutator_id.to_string()
        } else {
            mutator.to_string()
        }
    }

    pub fn mutator_display_name_en_with_dictionary(
        mutator: &str,
        dictionary: &Sc2DictionaryData,
    ) -> String {
        let mutator_id = TauriOverlayOps::canonical_mutator_id_with_dictionary(mutator, dictionary);
        dictionary
            .mutator_data(&mutator_id)
            .map(|value| TauriOverlayOps::decode_html_entities(&value.name.en))
            .filter(|value| !value.is_empty())
            .or_else(|| {
                dictionary
                    .mutator_ids
                    .get(&mutator_id)
                    .map(|value| value.to_string())
            })
            .unwrap_or_default()
    }

    fn infer_owner_handle_from_replay_path(path: &str) -> Option<String> {
        let replay_path = Path::new(path);
        for component in replay_path.components() {
            let raw = component.as_os_str().to_str()?;
            let normalized = ReplayAnalysis::normalized_handle_key(raw);
            if !normalized.is_empty() {
                return Some(normalized);
            }
        }
        None
    }

    pub fn replay_should_swap_main_and_ally(
        replay: &ReplayInfo,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
    ) -> bool {
        let p1_handle = ReplayAnalysis::normalized_handle_key(&replay.main().handle);
        let p2_handle = ReplayAnalysis::normalized_handle_key(&replay.ally().handle);
        if !main_handles.is_empty() && (!p1_handle.is_empty() || !p2_handle.is_empty()) {
            let p1_is_main =
                ReplayAnalysis::is_main_player_by_handle(&replay.main().handle, main_handles);
            let p2_is_main =
                ReplayAnalysis::is_main_player_by_handle(&replay.ally().handle, main_handles);
            if p1_is_main != p2_is_main {
                return !p1_is_main && p2_is_main;
            }
        }

        if let Some(owner_handle) =
            TauriOverlayOps::infer_owner_handle_from_replay_path(&replay.file)
        {
            let p1_owner = !p1_handle.is_empty() && p1_handle == owner_handle;
            let p2_owner = !p2_handle.is_empty() && p2_handle == owner_handle;
            if p1_owner != p2_owner {
                return !p1_owner && p2_owner;
            }
        }

        if !main_names.is_empty() {
            let p1_is_main =
                ReplayAnalysis::is_main_player_by_name(&replay.main().name, main_names);
            let p2_is_main =
                ReplayAnalysis::is_main_player_by_name(&replay.ally().name, main_names);
            if p1_is_main != p2_is_main {
                return !p1_is_main && p2_is_main;
            }
        }

        false
    }

    pub fn swap_player_stats_sides(value: &mut Value) {
        let Some(obj) = value.as_object_mut() else {
            return;
        };
        let one = obj.remove("1");
        let two = obj.remove("2");
        if let Some(v2) = two {
            obj.insert("1".to_string(), v2);
        }
        if let Some(v1) = one {
            obj.insert("2".to_string(), v1);
        }
    }

    pub fn canonicalize_coop_map_id(raw: &str) -> Option<String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else if trimmed.starts_with("AC_") {
            Some(trimmed.to_string())
        } else {
            None
        }
    }

    pub fn map_display_name(raw: &str) -> String {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            raw.to_string()
        } else {
            trimmed.to_string()
        }
    }

    pub fn sanitize_replay_text(value: &str) -> String {
        fn strip_tags(value: &str) -> String {
            let mut output = String::with_capacity(value.len());
            let mut in_tag = false;
            for ch in value.chars() {
                match ch {
                    '<' => {
                        in_tag = true;
                        output.push(' ');
                    }
                    '>' if in_tag => {
                        in_tag = false;
                        output.push(' ');
                    }
                    _ if !in_tag => output.push(ch),
                    _ => {}
                }
            }
            output
        }

        fn decode_html_entities(value: &str) -> String {
            let mut output = String::with_capacity(value.len());
            let mut chars = value.chars().peekable();

            while let Some(ch) = chars.next() {
                if ch != '&' {
                    output.push(ch);
                    continue;
                }

                let mut entity = String::from('&');
                while let Some(&next) = chars.peek() {
                    entity.push(next);
                    let _ = chars.next();
                    if next == ';' {
                        break;
                    }
                }

                let lower = entity.to_ascii_lowercase();
                let decoded = match lower.as_str() {
                    "&lt;" => "<".to_string(),
                    "&gt;" => ">".to_string(),
                    "&amp;" => "&".to_string(),
                    "&quot;" => "\"".to_string(),
                    "&apos;" | "&#39;" => "'".to_string(),
                    "&nbsp;" => " ".to_string(),
                    _ if lower.starts_with("&#x") && lower.ends_with(';') => {
                        u32::from_str_radix(&lower[3..lower.len() - 1], 16)
                            .ok()
                            .and_then(std::char::from_u32)
                            .map(|ch| ch.to_string())
                            .unwrap_or(entity)
                    }
                    _ if lower.starts_with("&#") && lower.ends_with(';') => lower
                        [2..lower.len() - 1]
                        .parse::<u32>()
                        .ok()
                        .and_then(std::char::from_u32)
                        .map(|ch| ch.to_string())
                        .unwrap_or(entity),
                    _ => entity,
                };

                output.push_str(&decoded);
            }

            output
        }

        let mut text = value
            .trim()
            .trim_matches('\u{0}')
            .replace("\\u003c", "<")
            .replace("\\u003e", ">");
        text = decode_html_entities(&text);
        text = text.replace("\\u003c", "<").replace("\\u003e", ">");
        text = strip_tags(&text);

        let mut normalized = String::with_capacity(text.len());
        let mut last_space = false;
        for ch in text.chars() {
            if ch.is_control() && ch != '\t' && ch != '\n' && ch != '\r' {
                if !last_space {
                    normalized.push(' ');
                    last_space = true;
                }
                continue;
            }
            if ch == ' ' {
                if !last_space {
                    normalized.push(' ');
                    last_space = true;
                }
                continue;
            }
            last_space = false;
            normalized.push(ch);
        }

        normalized.trim().to_string()
    }

    pub fn normalize_mastery_values(raw: &[u64]) -> Vec<u64> {
        let mut values = vec![0u64; 6];
        for (index, value) in raw.iter().take(6).enumerate() {
            values[index] = *value;
        }
        values
    }

    pub fn sanitize_unit_map(value: &Value) -> Value {
        if let Value::Object(raw) = value {
            let mut output = Map::new();
            for (key, raw_entry) in raw.iter() {
                if key.is_empty() {
                    continue;
                }
                if let Some(arr) = raw_entry.as_array() {
                    let mut values: [Value; 4] = [
                        Value::from(0),
                        Value::from(0),
                        Value::from(0),
                        Value::from(0.0),
                    ];
                    for (idx, item) in arr.iter().take(4).enumerate() {
                        if idx < 3 {
                            if let Some(number) = item.as_f64() {
                                values[idx] = if number.is_finite() {
                                    Value::from(number.round() as i64)
                                } else {
                                    Value::from(0)
                                };
                            } else if item.is_string() {
                                values[idx] = item.clone();
                            }
                        } else if let Some(number) = item.as_f64() {
                            values[idx] = if number.is_finite() {
                                Value::from(number.max(0.0))
                            } else {
                                Value::from(0.0)
                            };
                        }
                    }
                    output.insert(
                        TauriOverlayOps::sanitize_replay_text(key),
                        Value::Array(vec![
                            values[0].clone(),
                            values[1].clone(),
                            values[2].clone(),
                            values[3].clone(),
                        ]),
                    );
                }
            }
            Value::Object(output)
        } else {
            Value::Object(Map::new())
        }
    }

    pub fn sanitize_icon_map(value: &Value) -> Value {
        let mut output = Map::new();
        if let Value::Object(raw) = value {
            for (key, raw_value) in raw.iter() {
                if key.is_empty() {
                    continue;
                }

                if key == "outlaws" {
                    if let Some(items) = raw_value.as_array() {
                        let outlaws = items
                            .iter()
                            .filter_map(Value::as_str)
                            .map(|name| name.to_string())
                            .collect::<Vec<_>>();
                        if !outlaws.is_empty() {
                            output.insert(
                                key.clone(),
                                Value::Array(outlaws.into_iter().map(Value::String).collect()),
                            );
                        }
                    }
                    continue;
                }

                if let Some(count) = raw_value.as_u64() {
                    output.insert(key.clone(), Value::from(count));
                }
            }
        }
        Value::Object(output)
    }

    pub fn sanitize_player_stats_payload(value: &Value) -> Value {
        let mut output = Map::new();
        if let Value::Object(players) = value {
            for (key, raw_player) in players.iter() {
                if let Some(raw_player) = raw_player.as_object() {
                    let sanitize_array = |entry: Option<&Vec<Value>>| -> Vec<f64> {
                        entry
                            .map(|entries| {
                                entries
                                    .iter()
                                    .filter_map(|value| value.as_f64())
                                    .map(|value| if value.is_finite() { value } else { 0.0 })
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default()
                    };

                    let kills = sanitize_array(raw_player.get("killed").and_then(Value::as_array));
                    let army = sanitize_array(raw_player.get("army").and_then(Value::as_array));
                    let supply = sanitize_array(raw_player.get("supply").and_then(Value::as_array));
                    let mining = sanitize_array(raw_player.get("mining").and_then(Value::as_array));
                    let name = raw_player
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    output.insert(
                        key.clone(),
                        TauriOverlayOps::to_json_value(crate::shared_types::ReplayPlayerSeries {
                            name: TauriOverlayOps::sanitize_replay_text(&name),
                            killed: kills,
                            army,
                            supply,
                            mining,
                        }),
                    );
                }
            }
        }
        Value::Object(output)
    }

    fn normalize_known_commander_name(name: &str) -> Option<&'static str> {
        match name.trim().to_ascii_lowercase().as_str() {
            "alarak" => Some("Alarak"),
            "artanis" => Some("Artanis"),
            "fenix" => Some("Fenix"),
            "karax" => Some("Karax"),
            "vorazun" => Some("Vorazun"),
            "zeratul" => Some("Zeratul"),
            "horner" | "han & horner" => Some("Han & Horner"),
            "mengsk" => Some("Mengsk"),
            "nova" => Some("Nova"),
            "raynor" => Some("Raynor"),
            "swann" => Some("Swann"),
            "tychus" => Some("Tychus"),
            "abathur" => Some("Abathur"),
            "dehaka" => Some("Dehaka"),
            "kerrigan" => Some("Kerrigan"),
            "stukov" => Some("Stukov"),
            "zagara" => Some("Zagara"),
            "stetmann" => Some("Stetmann"),
            _ => None,
        }
    }

    pub fn normalized_commander_name(commander: &str, _fallback: &str) -> String {
        let trimmed = commander.trim();
        if trimmed.is_empty() {
            String::new()
        } else {
            TauriOverlayOps::normalize_known_commander_name(trimmed)
                .unwrap_or(trimmed)
                .to_string()
        }
    }
}
