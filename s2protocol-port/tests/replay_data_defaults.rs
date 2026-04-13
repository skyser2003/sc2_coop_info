use s2protocol_port::{ReplayDetails, ReplayGameDescription, ReplayHeader};

#[test]
fn game_speed_related_defaults_are_concrete_values() {
    let header = ReplayHeader::default();
    assert!(!header.m_useScaledTime);

    let details = ReplayDetails::default();
    assert_eq!(details.m_gameSpeed, 4);

    let game_description = ReplayGameDescription::default();
    assert_eq!(game_description.m_gameSpeed, 4);
}
