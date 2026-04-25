use crate::replay_analysis::ReplayAnalysis;
use serde_json::Value;
use std::collections::HashSet;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

#[derive(Debug, Clone)]
struct LiveGamePlayer {
    id: u64,
    name: String,
    kind: String,
    handle: String,
}

impl LiveGamePlayer {
    fn new(
        id: u64,
        name: impl Into<String>,
        kind: impl Into<String>,
        handle: impl Into<String>,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            kind: kind.into(),
            handle: handle.into(),
        }
    }

    fn id(&self) -> u64 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> &str {
        &self.kind
    }

    fn handle(&self) -> &str {
        &self.handle
    }
}

pub(crate) struct LiveGameOps;

impl LiveGameOps {
    pub(crate) fn value_as_u64_lossy(value: Option<&Value>) -> Option<u64> {
        value
            .and_then(Value::as_u64)
            .or_else(|| {
                value
                    .and_then(Value::as_i64)
                    .and_then(|entry| u64::try_from(entry).ok())
            })
            .or_else(|| {
                value
                    .and_then(Value::as_f64)
                    .filter(|entry| entry.is_finite() && *entry >= 0.0)
                    .map(|entry| entry.floor() as u64)
            })
    }

    pub(crate) fn fetch_sc2_live_game_payload() -> Option<Value> {
        let mut stream = TcpStream::connect("127.0.0.1:6119").ok()?;
        let _ = stream.set_read_timeout(Some(Duration::from_millis(800)));
        let _ = stream.set_write_timeout(Some(Duration::from_millis(800)));
        let request = b"GET /game HTTP/1.1\r\nHost: localhost:6119\r\nConnection: close\r\n\r\n";
        stream.write_all(request).ok()?;

        let mut response = Vec::<u8>::new();
        stream.read_to_end(&mut response).ok()?;
        let header_end = response
            .windows(4)
            .position(|window| window == b"\r\n\r\n")?;
        let body = response.get((header_end + 4)..)?;
        serde_json::from_slice::<Value>(body).ok()
    }

    pub(crate) fn extract_live_game_players(payload: &Value) -> usize {
        Self::parse_live_game_players(payload).len()
    }

    pub(crate) fn choose_other_coop_player_stats(
        payload: &Value,
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
    ) -> Option<(String, String)> {
        let players = Self::parse_live_game_players(payload);
        Self::choose_other_coop_player_stats_from_players(&players, main_names, main_handles)
    }

    fn parse_live_game_players(payload: &Value) -> Vec<LiveGamePlayer> {
        let Some(players) = payload.get("players").and_then(Value::as_array) else {
            return Vec::new();
        };

        players
            .iter()
            .filter_map(|player| {
                let as_object = player.as_object()?;
                let id = Self::value_as_u64_lossy(as_object.get("id"))
                    .or_else(|| Self::value_as_u64_lossy(as_object.get("playerId")))
                    .or_else(|| Self::value_as_u64_lossy(as_object.get("m_playerId")))
                    .unwrap_or(0);

                let name = as_object
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                let kind = as_object
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim()
                    .to_ascii_lowercase();
                let handle = as_object
                    .get("handle")
                    .or_else(|| as_object.get("toonHandle"))
                    .or_else(|| as_object.get("toon_handle"))
                    .or_else(|| as_object.get("battleTag"))
                    .or_else(|| as_object.get("battletag"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim()
                    .to_string();

                Some(LiveGamePlayer::new(id, name, kind, handle))
            })
            .collect()
    }

    fn choose_other_coop_player_stats_from_players(
        players: &[LiveGamePlayer],
        main_names: &HashSet<String>,
        main_handles: &HashSet<String>,
    ) -> Option<(String, String)> {
        let coop_players: Vec<&LiveGamePlayer> = players
            .iter()
            .filter(|player| player.id() == 1 || player.id() == 2)
            .filter(|player| !player.kind().eq_ignore_ascii_case("computer"))
            .collect();
        if coop_players.is_empty() {
            return None;
        }

        let mut main_marked_count = 0usize;
        let mut other_candidates = Vec::<&LiveGamePlayer>::new();
        for player in coop_players.iter() {
            let is_main = ReplayAnalysis::is_main_player_identity(
                player.name(),
                player.handle(),
                main_names,
                main_handles,
            );
            if is_main {
                main_marked_count += 1;
            } else {
                other_candidates.push(*player);
            }
        }

        if main_marked_count > 0 && !other_candidates.is_empty() {
            other_candidates.sort_by_key(|player| player.id());

            return other_candidates
                .into_iter()
                .map(|player| {
                    let name = player.name().trim();
                    let handle = player.handle().to_string();

                    (handle, name.to_string())
                })
                .next();
        }

        None
    }

    pub(crate) fn all_players_are_users(payload: &Value) -> bool {
        let players = Self::parse_live_game_players(payload);
        players
            .iter()
            .all(|player| player.kind().eq_ignore_ascii_case("user"))
    }
}
