use sco_tauri_overlay::test_helper::test_replay_path;
use sco_tauri_overlay::{ReplayChatMessage, ReplayInfo, ReplayPlayerInfo};

#[test]
fn replay_chat_payload_uses_slot_names_and_sanitizes_messages() {
    let mut replay = ReplayInfo::with_players(
        ReplayPlayerInfo {
            name: "Slot One".to_string(),
            ..ReplayPlayerInfo::default()
        },
        ReplayPlayerInfo {
            name: "Slot Two".to_string(),
            ..ReplayPlayerInfo::default()
        },
        0,
    );
    replay.file = test_replay_path("chat.SC2Replay");
    replay.date = 1_710_000_000;
    replay.map = "Void Launch".to_string();
    replay.result = "Victory".to_string();
    replay.messages = vec![
        ReplayChatMessage {
            player: 1,
            text: "<span>Hello</span>".to_string(),
            time: 15.9,
        },
        ReplayChatMessage {
            player: 2,
            text: "gg".to_string(),
            time: -5.0,
        },
    ];
    let payload = replay.chat_payload();

    assert_eq!(payload.slot1_name, "Slot One");
    assert_eq!(payload.slot2_name, "Slot Two");
    assert_eq!(payload.messages.len(), 2);
    assert_eq!(payload.messages[0].text, "Hello");
    assert_eq!(payload.messages[0].time, 15.9);
    assert_eq!(payload.messages[1].text, "gg");
    assert_eq!(payload.messages[1].time, 0.0);
}

#[test]
fn replay_chat_payload_returns_empty_slot_names_when_slot_names_are_missing() {
    let mut replay = ReplayInfo::with_players(
        ReplayPlayerInfo {
            ..ReplayPlayerInfo::default()
        },
        ReplayPlayerInfo {
            ..ReplayPlayerInfo::default()
        },
        0,
    );
    replay.file = test_replay_path("fallback.SC2Replay");
    replay.messages = vec![ReplayChatMessage {
        player: 1,
        text: "ready".to_string(),
        time: 1.0,
    }];

    let payload = replay.chat_payload();

    assert_eq!(payload.slot1_name, "");
    assert_eq!(payload.slot2_name, "");
}
