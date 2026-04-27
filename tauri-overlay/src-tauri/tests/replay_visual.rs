use s2protocol_port::{ReplayEvent, TrackerEvent};
use sco_tauri_overlay::{
    ReplayVisualBuildInput, ReplayVisualDictionaries, ReplayVisualOps, ReplayVisualOwnerKind,
    ReplayVisualPlayer, ReplayVisualUnitGroup,
};
use std::collections::{HashMap, HashSet};

fn tracker_event(event_name: &str, game_loop: i64) -> TrackerEvent {
    TrackerEvent {
        event: event_name.to_string(),
        game_loop,
        ..TrackerEvent::default()
    }
}

fn unit_born(
    game_loop: i64,
    tag_index: i64,
    recycle: i64,
    unit_type: &str,
    player_id: i64,
    x: i64,
    y: i64,
) -> ReplayEvent {
    let mut event = tracker_event("NNet.Replay.Tracker.SUnitBornEvent", game_loop);
    event.m_unit_tag_index = Some(tag_index);
    event.m_unit_tag_recycle = Some(recycle);
    event.m_unit_type_name = Some(unit_type.to_string());
    event.m_control_player_id = Some(player_id);
    event.m_x = Some(x);
    event.m_y = Some(y);
    ReplayEvent::Tracker(event)
}

fn unit_positions(game_loop: i64, first_unit_index: i64, items: Vec<i64>) -> ReplayEvent {
    let mut event = tracker_event("NNet.Replay.Tracker.SUnitPositionsEvent", game_loop);
    event.m_first_unit_index = Some(first_unit_index);
    event.m_position_items = items;
    ReplayEvent::Tracker(event)
}

fn visual_dictionaries() -> ReplayVisualDictionaries {
    ReplayVisualDictionaries::new(
        HashMap::from([
            ("CommandCenter".to_string(), "Command Center".to_string()),
            ("PhotonCannon".to_string(), "Photon Cannon".to_string()),
            ("Marine".to_string(), "Marine".to_string()),
        ]),
        HashSet::from(["Marine".to_string()]),
        HashSet::from([3_i64, 4_i64]),
    )
}

fn visual_input() -> ReplayVisualBuildInput {
    ReplayVisualBuildInput::new(
        "synthetic.SC2Replay",
        "Void Launch",
        "Victory",
        180.0,
        200.0,
        200.0,
        vec![
            ReplayVisualPlayer {
                player_id: 1,
                label: "Main".to_string(),
                owner_kind: ReplayVisualOwnerKind::Main,
                color: "#38bdf8".to_string(),
            },
            ReplayVisualPlayer {
                player_id: 2,
                label: "Ally".to_string(),
                owner_kind: ReplayVisualOwnerKind::Ally,
                color: "#22c55e".to_string(),
            },
            ReplayVisualPlayer {
                player_id: 3,
                label: "Amon".to_string(),
                owner_kind: ReplayVisualOwnerKind::Amon,
                color: "#ef4444".to_string(),
            },
        ],
        1,
    )
}

#[test]
fn visual_payload_groups_units_and_tracks_positions() {
    let events = vec![
        unit_born(0, 1, 1, "CommandCenter", 1, 20, 20),
        unit_born(0, 2, 1, "PhotonCannon", 1, 24, 20),
        unit_born(0, 3, 1, "Marine", 1, 30, 20),
        unit_positions(80, 1, vec![0, 21, 21, 1, 25, 21, 1, 31, 21]),
    ];

    let payload =
        ReplayVisualOps::payload_from_events(visual_input(), visual_dictionaries(), &events);
    let final_frame = payload.frames.last().expect("visual frame");
    let frame_loops = payload
        .frames
        .iter()
        .map(|frame| frame.game_loop)
        .collect::<Vec<_>>();

    assert_eq!(payload.map, "Void Launch");
    assert_eq!(frame_loops, vec![0, 16, 32, 48, 64, 80]);
    assert_eq!(final_frame.units.len(), 3);
    assert!(final_frame.units.iter().any(|unit| {
        unit.unit_type == "CommandCenter"
            && unit.group == ReplayVisualUnitGroup::Buildings
            && unit.x == 21.0
            && unit.y == 21.0
    }));
    assert!(final_frame.units.iter().any(|unit| {
        unit.unit_type == "PhotonCannon" && unit.group == ReplayVisualUnitGroup::DefenseBuildings
    }));
    assert!(final_frame.units.iter().any(|unit| {
        unit.unit_type == "Marine" && unit.group == ReplayVisualUnitGroup::AttackUnits
    }));
}

#[test]
fn visual_payload_groups_enemy_assaults_from_wave_units() {
    let events = vec![
        unit_born(1000, 10, 1, "Marine", 3, 50, 60),
        unit_born(1000, 11, 1, "Marine", 3, 52, 61),
        unit_born(1000, 12, 1, "Marine", 3, 54, 62),
        unit_born(1000, 13, 1, "Marine", 3, 56, 63),
        unit_born(1000, 14, 1, "Marine", 3, 58, 64),
        unit_born(1000, 15, 1, "Marine", 3, 60, 65),
    ];

    let payload =
        ReplayVisualOps::payload_from_events(visual_input(), visual_dictionaries(), &events);
    let final_frame = payload.frames.last().expect("visual frame");

    assert_eq!(payload.assaults.len(), 1);
    assert_eq!(payload.assaults[0].unit_count, 6);
    assert_eq!(payload.assaults[0].units[0].display_name, "Marine");
    assert_eq!(payload.assaults[0].units[0].count, 6);
    assert_eq!(
        final_frame
            .units
            .iter()
            .filter(|unit| unit.group == ReplayVisualUnitGroup::EnemyAssaults)
            .count(),
        6
    );
}
