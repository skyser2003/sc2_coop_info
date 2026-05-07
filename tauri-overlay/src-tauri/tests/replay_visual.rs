use s2protocol_port::{
    AbilityData, CmdEventData, GameEvent, ReplayEvent, SelectionDeltaData, SelectionRemoveMask,
    SnapshotPoint, SnapshotPointValue, TrackerEvent, UnitTag,
};
use sco_tauri_overlay::{
    ReplayVisualBuildInput, ReplayVisualDictionaries, ReplayVisualMapSize, ReplayVisualOps,
    ReplayVisualOwnerKind, ReplayVisualPlayer, ReplayVisualReplayInfo, ReplayVisualUnitGroup,
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

fn deep_tunnel_command(game_loop: i64, user_id: i64, x: i64, y: i64) -> ReplayEvent {
    target_point_command(game_loop, user_id, 2307, x, y)
}

fn tychus_medivac_command(game_loop: i64, user_id: i64, x: i64, y: i64) -> ReplayEvent {
    target_point_command(game_loop, user_id, 3101, x, y)
}

fn target_point_command(
    game_loop: i64,
    user_id: i64,
    ability_link: i64,
    x: i64,
    y: i64,
) -> ReplayEvent {
    ReplayEvent::Game(GameEvent {
        event: "NNet.Game.SCmdEvent".to_string(),
        game_loop,
        user_id: Some(user_id),
        m_abil: Some(AbilityData {
            m_abilLink: ability_link,
            ..AbilityData::default()
        }),
        m_data: Some(CmdEventData {
            TargetPoint: Some(SnapshotPoint {
                values: vec![
                    SnapshotPointValue::Int(x * 4096),
                    SnapshotPointValue::Int(y * 4096),
                    SnapshotPointValue::Int(0),
                ],
            }),
            TargetUnit: None,
        }),
        ..GameEvent::default()
    })
}

fn selection_delta(game_loop: i64, user_id: i64, unit_tags: Vec<i64>) -> ReplayEvent {
    selection_delta_with_remove_mask(game_loop, user_id, SelectionRemoveMask::None, unit_tags)
}

fn selection_delta_with_remove_mask(
    game_loop: i64,
    user_id: i64,
    remove_mask: SelectionRemoveMask,
    unit_tags: Vec<i64>,
) -> ReplayEvent {
    ReplayEvent::Game(GameEvent {
        event: "NNet.Game.SSelectionDeltaEvent".to_string(),
        game_loop,
        user_id: Some(user_id),
        m_delta: Some(SelectionDeltaData {
            m_remove_mask: remove_mask,
            m_add_unit_tags: unit_tags,
            ..SelectionDeltaData::default()
        }),
        ..GameEvent::default()
    })
}

fn game_unit_tag(tag_index: i64, recycle: i64) -> i64 {
    i64::try_from(UnitTag::from_parts(tag_index.into(), recycle.into()))
        .expect("synthetic unit tag should fit in i64")
}

fn visual_dictionaries() -> ReplayVisualDictionaries {
    ReplayVisualDictionaries::new(
        HashMap::from([
            ("CommandCenter".to_string(), "Command Center".to_string()),
            ("PhotonCannon".to_string(), "Photon Cannon".to_string()),
            ("Marine".to_string(), "Marine".to_string()),
            ("Brutalisk".to_string(), "Brutalisk".to_string()),
            (
                "AbathurSymbioteBrutalisk".to_string(),
                "Brutalisk's Symbiote".to_string(),
            ),
            (
                "AbathurSymbioteLeviathan".to_string(),
                "Leviathan's Symbiote".to_string(),
            ),
            ("CreepTumorStukov".to_string(), "Creep Tumor".to_string()),
            (
                "CoopCasterAbathur".to_string(),
                "Abathur's Top Bar".to_string(),
            ),
            (
                "CoopCasterAlarak".to_string(),
                "Photon Overcharge".to_string(),
            ),
            (
                "FenixManaDummy1".to_string(),
                "Fenix Mana Dummy".to_string(),
            ),
            ("TychusCoop".to_string(), "Tychus Findlay".to_string()),
            ("TychusSpectre".to_string(), "Nux".to_string()),
            ("TychusSCV".to_string(), "Tychus SCV".to_string()),
            (
                "TychusEngineeringBay".to_string(),
                "Engineering Bay".to_string(),
            ),
            ("SuperWarpGate".to_string(), "Super Warp Gate".to_string()),
        ]),
        HashSet::from(["Marine".to_string()]),
        HashSet::from([3_i64, 4_i64]),
    )
}

fn visual_dictionaries_with_omitted_units() -> ReplayVisualDictionaries {
    ReplayVisualDictionaries::new_with_omitted_units(
        HashMap::from([
            ("Marine".to_string(), "Marine".to_string()),
            ("SuperWarpGate".to_string(), "Super Warp Gate".to_string()),
        ]),
        HashSet::from(["Marine".to_string()]),
        HashSet::from([3_i64, 4_i64]),
        HashSet::from(["SuperWarpGate".to_string()]),
    )
}

fn visual_input() -> ReplayVisualBuildInput {
    ReplayVisualBuildInput::new(
        ReplayVisualReplayInfo::new("synthetic.SC2Replay", "Void Launch", "Victory", 180.0),
        ReplayVisualMapSize::new(200.0, 200.0),
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

#[test]
fn visual_payload_omits_visualizer_filtered_units_before_rendering() {
    let events = vec![
        unit_born(0, 1, 1, "CoopCasterAbathur", 1, 0, 0),
        unit_born(0, 2, 1, "CoopCasterAlarak", 1, 0, 0),
        unit_born(0, 3, 1, "FenixManaDummy1", 1, 0, 0),
        unit_born(0, 4, 1, "AbathurSymbioteBrutalisk", 1, 0, 0),
        unit_born(0, 5, 1, "AbathurSymbioteLeviathan", 1, 0, 0),
        unit_born(0, 6, 1, "CreepTumorStukov", 1, 0, 0),
        unit_born(0, 7, 1, "Marine", 1, 30, 20),
        unit_positions(
            80,
            1,
            vec![
                0, 5, 5, 1, 6, 6, 1, 7, 7, 1, 8, 8, 1, 9, 9, 1, 10, 10, 1, 31, 21,
            ],
        ),
    ];

    let payload =
        ReplayVisualOps::payload_from_events(visual_input(), visual_dictionaries(), &events);
    let final_frame = payload.frames.last().expect("visual frame");

    assert_eq!(final_frame.units.len(), 1);
    assert_eq!(final_frame.units[0].unit_type, "Marine");
    assert_eq!(final_frame.units[0].x, 31.0);
    assert_eq!(final_frame.units[0].y, 21.0);
}

#[test]
fn visual_payload_omits_dictionary_excluded_units_before_rendering() {
    let events = vec![
        unit_born(0, 1, 1, "SuperWarpGate", 3, 0, 0),
        unit_born(0, 2, 1, "Marine", 3, 30, 20),
        unit_positions(80, 1, vec![0, 5, 5, 1, 31, 21]),
    ];

    let payload = ReplayVisualOps::payload_from_events(
        visual_input(),
        visual_dictionaries_with_omitted_units(),
        &events,
    );
    let final_frame = payload.frames.last().expect("visual frame");

    assert_eq!(final_frame.units.len(), 1);
    assert_eq!(final_frame.units[0].unit_type, "Marine");
    assert_eq!(final_frame.units[0].x, 31.0);
    assert_eq!(final_frame.units[0].y, 21.0);
}

#[test]
fn visual_payload_marks_brutalisk_deep_tunnel_as_one_second_movement() {
    let events = vec![
        unit_born(0, 1, 1, "Brutalisk", 1, 20, 20),
        unit_born(0, 2, 1, "Brutalisk", 1, 40, 40),
        unit_born(0, 3, 1, "Marine", 1, 25, 25),
        selection_delta(10, 0, vec![game_unit_tag(1, 1)]),
        deep_tunnel_command(20, 0, 60, 70),
        unit_positions(32, 1, vec![0, 20, 20, 1, 40, 40, 1, 25, 25]),
        unit_positions(48, 1, vec![0, 61, 70, 1, 40, 40, 1, 25, 25]),
    ];

    let payload =
        ReplayVisualOps::payload_from_events(visual_input(), visual_dictionaries(), &events);
    let command_frame = payload
        .frames
        .iter()
        .find(|frame| frame.game_loop == 20)
        .expect("deep tunnel command frame should be captured");
    let brutalisk = command_frame
        .units
        .iter()
        .find(|unit| unit.id == "100001")
        .expect("selected brutalisk should remain visible");
    let idle_brutalisk = command_frame
        .units
        .iter()
        .find(|unit| unit.id == "100002")
        .expect("idle brutalisk should remain visible");
    let marine = command_frame
        .units
        .iter()
        .find(|unit| unit.unit_type == "Marine")
        .expect("marine should remain visible");

    assert_eq!(brutalisk.x, 20.0);
    assert_eq!(brutalisk.y, 20.0);
    assert!(brutalisk.interpolate_from_previous);
    assert_eq!(idle_brutalisk.x, 40.0);
    assert_eq!(idle_brutalisk.y, 40.0);
    assert!(idle_brutalisk.interpolate_from_previous);
    assert_eq!(marine.x, 25.0);
    assert_eq!(marine.y, 25.0);
    assert!(marine.interpolate_from_previous);

    assert!(
        !payload.frames.iter().any(|frame| frame.game_loop == 32),
        "stale tracker position should not create a frame before Deep Tunnel finishes"
    );

    let movement_frame = payload
        .frames
        .iter()
        .find(|frame| frame.game_loop == 36)
        .expect("deep tunnel movement frame should be captured");
    let moved_brutalisk = movement_frame
        .units
        .iter()
        .find(|unit| unit.id == "100001")
        .expect("selected brutalisk should remain visible");
    assert_eq!(moved_brutalisk.x, 60.0);
    assert_eq!(moved_brutalisk.y, 70.0);
    assert!(moved_brutalisk.interpolate_from_previous);
}

#[test]
fn visual_payload_applies_deep_tunnel_commands_per_unit() {
    let events = vec![
        unit_born(0, 1, 1, "Brutalisk", 1, 20, 20),
        unit_born(0, 2, 1, "Brutalisk", 1, 22, 20),
        selection_delta(10, 0, vec![game_unit_tag(1, 1), game_unit_tag(2, 1)]),
        deep_tunnel_command(20, 0, 60, 70),
        deep_tunnel_command(44, 0, 80, 90),
        unit_positions(80, 1, vec![0, 61, 70, 1, 80, 90]),
    ];

    let payload =
        ReplayVisualOps::payload_from_events(visual_input(), visual_dictionaries(), &events);
    let first_arrival_frame = payload
        .frames
        .iter()
        .find(|frame| frame.game_loop == 36)
        .expect("first Deep Tunnel arrival frame should be backpatched");
    let second_command_frame = payload
        .frames
        .iter()
        .find(|frame| frame.game_loop == 44)
        .expect("second Deep Tunnel command frame should be backpatched");
    let second_arrival_frame = payload
        .frames
        .iter()
        .find(|frame| frame.game_loop == 60)
        .expect("second Deep Tunnel arrival frame should be backpatched");

    let first_arrival_unit = first_arrival_frame
        .units
        .iter()
        .find(|unit| unit.id == "100001")
        .expect("first Brutalisk should remain visible");
    let second_unit_before_arrival = first_arrival_frame
        .units
        .iter()
        .find(|unit| unit.id == "100002")
        .expect("second Brutalisk should remain visible");
    assert_eq!(first_arrival_unit.x, 60.0);
    assert_eq!(first_arrival_unit.y, 70.0);
    assert!(first_arrival_unit.interpolate_from_previous);
    assert_eq!(second_unit_before_arrival.x, 22.0);
    assert_eq!(second_unit_before_arrival.y, 20.0);

    let first_unit_during_second_command = second_command_frame
        .units
        .iter()
        .find(|unit| unit.id == "100001")
        .expect("first Brutalisk should remain visible");
    let second_unit_during_second_command = second_command_frame
        .units
        .iter()
        .find(|unit| unit.id == "100002")
        .expect("second Brutalisk should remain visible");
    assert_eq!(first_unit_during_second_command.x, 60.0);
    assert_eq!(first_unit_during_second_command.y, 70.0);
    assert_eq!(second_unit_during_second_command.x, 22.0);
    assert_eq!(second_unit_during_second_command.y, 20.0);

    let first_unit_after_second_arrival = second_arrival_frame
        .units
        .iter()
        .find(|unit| unit.id == "100001")
        .expect("first Brutalisk should remain visible");
    let second_unit_after_second_arrival = second_arrival_frame
        .units
        .iter()
        .find(|unit| unit.id == "100002")
        .expect("second Brutalisk should remain visible");
    assert_eq!(first_unit_after_second_arrival.x, 60.0);
    assert_eq!(first_unit_after_second_arrival.y, 70.0);
    assert_eq!(second_unit_after_second_arrival.x, 80.0);
    assert_eq!(second_unit_after_second_arrival.y, 90.0);
    assert!(second_unit_after_second_arrival.interpolate_from_previous);
}

#[test]
fn visual_payload_deep_tunnel_falls_back_when_selection_is_stale() {
    let events = vec![
        unit_born(0, 1, 1, "Brutalisk", 1, 20, 20),
        unit_born(0, 2, 1, "Marine", 1, 25, 25),
        selection_delta(10, 0, vec![game_unit_tag(2, 1)]),
        deep_tunnel_command(20, 0, 60, 70),
        unit_positions(32, 1, vec![0, 20, 20, 1, 25, 25]),
    ];

    let payload =
        ReplayVisualOps::payload_from_events(visual_input(), visual_dictionaries(), &events);
    let command_frame = payload
        .frames
        .iter()
        .find(|frame| frame.game_loop == 20)
        .expect("deep tunnel command frame should be captured");
    let brutalisk = command_frame
        .units
        .iter()
        .find(|unit| unit.unit_type == "Brutalisk")
        .expect("brutalisk should remain visible");
    let marine = command_frame
        .units
        .iter()
        .find(|unit| unit.unit_type == "Marine")
        .expect("marine should remain visible");

    assert_eq!(brutalisk.x, 20.0);
    assert_eq!(brutalisk.y, 20.0);
    assert!(brutalisk.interpolate_from_previous);
    assert_eq!(marine.x, 25.0);
    assert_eq!(marine.y, 25.0);
    assert!(marine.interpolate_from_previous);

    assert!(
        !payload.frames.iter().any(|frame| frame.game_loop == 32),
        "stale tracker position should not create a frame before Deep Tunnel finishes"
    );

    let movement_frame = payload
        .frames
        .iter()
        .find(|frame| frame.game_loop == 36)
        .expect("deep tunnel movement frame should be captured");
    let moved_brutalisk = movement_frame
        .units
        .iter()
        .find(|unit| unit.unit_type == "Brutalisk")
        .expect("brutalisk should remain visible");
    assert_eq!(moved_brutalisk.x, 60.0);
    assert_eq!(moved_brutalisk.y, 70.0);
    assert!(moved_brutalisk.interpolate_from_previous);
}

#[test]
fn visual_payload_moves_selected_tychus_medivac_passengers_only() {
    let events = vec![
        unit_born(0, 1, 1, "TychusCoop", 1, 10, 10),
        unit_born(0, 2, 1, "TychusSpectre", 1, 12, 12),
        unit_born(0, 3, 1, "TychusSCV", 1, 14, 14),
        unit_born(0, 4, 1, "TychusEngineeringBay", 1, 16, 16),
        unit_born(0, 5, 1, "Marine", 1, 18, 18),
        selection_delta(10, 0, vec![game_unit_tag(1, 1)]),
        selection_delta_with_remove_mask(
            12,
            0,
            SelectionRemoveMask::OneIndices(vec![0]),
            vec![game_unit_tag(4, 1)],
        ),
        tychus_medivac_command(20, 0, 80, 90),
        unit_positions(
            32,
            1,
            vec![0, 10, 10, 1, 12, 12, 1, 14, 14, 1, 16, 16, 1, 18, 18],
        ),
        unit_positions(
            48,
            1,
            vec![0, 81, 90, 1, 12, 12, 1, 14, 14, 1, 16, 16, 1, 18, 18],
        ),
    ];

    let payload =
        ReplayVisualOps::payload_from_events(visual_input(), visual_dictionaries(), &events);
    let command_frame = payload
        .frames
        .iter()
        .find(|frame| frame.game_loop == 20)
        .expect("medivac command frame should be captured");
    let tychus = command_frame
        .units
        .iter()
        .find(|unit| unit.unit_type == "TychusCoop")
        .expect("Tychus should remain visible");
    let nux = command_frame
        .units
        .iter()
        .find(|unit| unit.unit_type == "TychusSpectre")
        .expect("Nux should remain visible");
    let scv = command_frame
        .units
        .iter()
        .find(|unit| unit.unit_type == "TychusSCV")
        .expect("SCV should remain visible");
    let engineering_bay = command_frame
        .units
        .iter()
        .find(|unit| unit.unit_type == "TychusEngineeringBay")
        .expect("engineering bay should remain visible");
    let marine = command_frame
        .units
        .iter()
        .find(|unit| unit.unit_type == "Marine")
        .expect("marine should remain visible");

    assert_eq!(tychus.x, 80.0);
    assert_eq!(tychus.y, 90.0);
    assert!(!tychus.interpolate_from_previous);
    assert_eq!(nux.x, 12.0);
    assert_eq!(nux.y, 12.0);
    assert!(nux.interpolate_from_previous);
    assert_eq!(scv.x, 14.0);
    assert_eq!(scv.y, 14.0);
    assert_eq!(engineering_bay.x, 16.0);
    assert_eq!(engineering_bay.y, 16.0);
    assert_eq!(marine.x, 18.0);
    assert_eq!(marine.y, 18.0);

    let stale_tracker_frame = payload
        .frames
        .iter()
        .find(|frame| frame.game_loop == 32)
        .expect("stale tracker frame should be captured");
    let stale_frame_tychus = stale_tracker_frame
        .units
        .iter()
        .find(|unit| unit.unit_type == "TychusCoop")
        .expect("Tychus should remain visible");
    let stale_frame_nux = stale_tracker_frame
        .units
        .iter()
        .find(|unit| unit.unit_type == "TychusSpectre")
        .expect("Nux should remain visible");
    assert_eq!(stale_frame_tychus.x, 80.0);
    assert_eq!(stale_frame_tychus.y, 90.0);
    assert_eq!(stale_frame_nux.x, 12.0);
    assert_eq!(stale_frame_nux.y, 12.0);

    let accepted_tracker_frame = payload
        .frames
        .iter()
        .find(|frame| frame.game_loop == 48)
        .expect("accepted tracker frame should be captured");
    let accepted_frame_tychus = accepted_tracker_frame
        .units
        .iter()
        .find(|unit| unit.unit_type == "TychusCoop")
        .expect("Tychus should remain visible");
    let accepted_frame_nux = accepted_tracker_frame
        .units
        .iter()
        .find(|unit| unit.unit_type == "TychusSpectre")
        .expect("Nux should remain visible");
    assert_eq!(accepted_frame_tychus.x, 81.0);
    assert_eq!(accepted_frame_tychus.y, 90.0);
    assert_eq!(accepted_frame_nux.x, 12.0);
    assert_eq!(accepted_frame_nux.y, 12.0);
}

#[test]
fn visual_payload_marks_tracker_confirmed_tychus_medivac_jumps_as_snap_positions() {
    let events = vec![
        unit_born(0, 1, 1, "TychusCoop", 1, 10, 10),
        unit_born(0, 2, 1, "TychusSpectre", 1, 12, 12),
        unit_born(0, 3, 1, "TychusEngineeringBay", 1, 16, 16),
        selection_delta(10, 0, vec![game_unit_tag(3, 1)]),
        tychus_medivac_command(20, 0, 80, 90),
        unit_positions(32, 1, vec![0, 78, 88, 1, 40, 40, 1, 16, 16]),
    ];

    let payload =
        ReplayVisualOps::payload_from_events(visual_input(), visual_dictionaries(), &events);
    let command_frame = payload
        .frames
        .iter()
        .find(|frame| frame.game_loop == 20)
        .expect("medivac command frame should be backpatched");
    let command_frame_tychus = command_frame
        .units
        .iter()
        .find(|unit| unit.unit_type == "TychusCoop")
        .expect("Tychus should remain visible at the command loop");
    assert_eq!(command_frame_tychus.x, 78.0);
    assert_eq!(command_frame_tychus.y, 88.0);
    assert!(!command_frame_tychus.interpolate_from_previous);

    let tracker_frame = payload
        .frames
        .iter()
        .find(|frame| frame.game_loop == 32)
        .expect("tracker frame should be captured");
    let tychus = tracker_frame
        .units
        .iter()
        .find(|unit| unit.unit_type == "TychusCoop")
        .expect("Tychus should remain visible");
    let nux = tracker_frame
        .units
        .iter()
        .find(|unit| unit.unit_type == "TychusSpectre")
        .expect("Nux should remain visible");

    assert_eq!(tychus.x, 78.0);
    assert_eq!(tychus.y, 88.0);
    assert!(!tychus.interpolate_from_previous);
    assert_eq!(nux.x, 40.0);
    assert_eq!(nux.y, 40.0);
    assert!(nux.interpolate_from_previous);
}
