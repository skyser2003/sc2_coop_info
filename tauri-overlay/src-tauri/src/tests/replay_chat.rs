use super::*;

#[test]
fn replay_chat_payload_uses_slot_names_and_sanitizes_messages() {
    let replay = ReplayInfo {
        file: test_replay_path("chat.SC2Replay"),
        date: 1_710_000_000,
        map: "Void Launch".to_string(),
        result: "Victory".to_string(),
        p1: "Main".to_string(),
        p2: "Ally".to_string(),
        slot1_name: "Slot One".to_string(),
        slot2_name: "Slot Two".to_string(),
        messages: vec![
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
        ],
        ..ReplayInfo::default()
    };

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
fn replay_chat_payload_falls_back_to_player_names_when_slot_names_are_missing() {
    let replay = ReplayInfo {
        file: test_replay_path("fallback.SC2Replay"),
        p1: "Fallback Main".to_string(),
        p2: "Fallback Ally".to_string(),
        messages: vec![ReplayChatMessage {
            player: 1,
            text: "ready".to_string(),
            time: 1.0,
        }],
        ..ReplayInfo::default()
    };

    let payload = replay.chat_payload();

    assert_eq!(payload.slot1_name, "Fallback Main");
    assert_eq!(payload.slot2_name, "Fallback Ally");
}
