use s2coop_analyzer::detailed_replay_analysis::realtime_length_from_replay;
use s2protocol_port::{ReplayDetails, ReplayGameDescription, ReplayInitData, ReplaySyncLobbyState};

fn init_data_with_game_speed(game_speed: i64) -> ReplayInitData {
    ReplayInitData {
        m_syncLobbyState: ReplaySyncLobbyState {
            m_gameDescription: ReplayGameDescription {
                m_gameSpeed: game_speed,
                ..ReplayGameDescription::default()
            },
            ..ReplaySyncLobbyState::default()
        },
    }
}

fn details_with_game_speed(game_speed: i64) -> ReplayDetails {
    ReplayDetails {
        m_gameSpeed: game_speed,
        ..ReplayDetails::default()
    }
}

#[test]
fn realtime_length_prefers_details_game_speed() {
    let realtime_length = realtime_length_from_replay(
        560.0,
        &details_with_game_speed(4),
        &init_data_with_game_speed(2),
    );

    assert_eq!(realtime_length, 400.0);
}

#[test]
fn realtime_length_falls_back_to_init_data_game_speed_when_details_speed_is_invalid() {
    let realtime_length = realtime_length_from_replay(
        560.0,
        &details_with_game_speed(99),
        &init_data_with_game_speed(4),
    );

    assert_eq!(realtime_length, 400.0);
}

#[test]
fn realtime_length_defaults_to_faster_when_both_game_speed_codes_are_invalid() {
    let realtime_length = realtime_length_from_replay(
        560.0,
        &details_with_game_speed(99),
        &init_data_with_game_speed(99),
    );

    assert_eq!(realtime_length, 400.0);
}
