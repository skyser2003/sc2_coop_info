use s2coop_analyzer::detailed_replay_analysis::DetailedReplayAnalyzer;
use std::collections::HashSet;

#[test]
fn coop_plus_commander_name_uses_prefix_mastery_identifiers_only() {
    let cases = [
        ("MasteryMaarCopiesTime", "Maar"),
        ("MasteryZeratulArtifactFragmentSpawnRate", "Zeratul"),
    ];

    for (upgrade_name, expected_commander) in cases {
        assert_eq!(
            DetailedReplayAnalyzer::coop_plus_commander_name_from_upgrade(upgrade_name),
            Some(expected_commander.to_string())
        );
    }

    for nexus_identifier in ["SelendisMasterySeries1New", "StukovCommander"] {
        assert_eq!(
            DetailedReplayAnalyzer::coop_plus_commander_name_from_upgrade(nexus_identifier),
            None,
            "Co-op+ parser accepted Nexus identifier {nexus_identifier}"
        );
    }
}

#[test]
fn nexus_coop_commander_name_uses_nexus_identifiers_only() {
    let cases = [
        ("HansonMasterySpecialChange1", "Hanson"),
        ("RaynorMasteryUpgrade", "Raynor"),
        ("SelendisMasterySeries1New", "Selendis"),
        ("StukovCommander", "Stukov"),
        ("Ariel_Hanson-MasteryLevel", "Ariel_Hanson"),
    ];

    for (upgrade_name, expected_commander) in cases {
        assert_eq!(
            DetailedReplayAnalyzer::nexus_coop_commander_name_from_upgrade(upgrade_name),
            Some(expected_commander.to_string())
        );
    }

    for coop_plus_identifier in [
        "MasteryMaarCopiesTime",
        "MasteryZeratulArtifactFragmentSpawnRate",
    ] {
        assert_eq!(
            DetailedReplayAnalyzer::nexus_coop_commander_name_from_upgrade(coop_plus_identifier),
            None,
            "Nexus parser accepted Co-op+ identifier {coop_plus_identifier}"
        );
    }
}

#[test]
fn map_specific_commander_names_ignore_generic_and_unrelated_upgrades() {
    let upgrade_names = [
        "CommanderLevel",
        "MasteryLevel",
        "PlayerCommander",
        "GenericMasterySelection",
        "RaynorCommanderArmorVanadium",
        "SwannCommanderVehicleHealth",
        "CombatDrugs2",
        "123MasteryLevel",
    ];

    for upgrade_name in upgrade_names {
        assert_eq!(
            DetailedReplayAnalyzer::coop_plus_commander_name_from_upgrade(upgrade_name),
            None,
            "unexpected Co-op+ commander name from {upgrade_name}"
        );
        assert_eq!(
            DetailedReplayAnalyzer::nexus_coop_commander_name_from_upgrade(upgrade_name),
            None,
            "unexpected Nexus commander name from {upgrade_name}"
        );
    }
}

#[test]
fn numbered_unit_variant_preserves_its_parsed_name_for_known_commanders() {
    let known_commander_names = HashSet::from([
        "Stetmann".to_string(),
        "Stukov".to_string(),
        "Dehaka".to_string(),
    ]);

    assert_eq!(
        DetailedReplayAnalyzer::commander_name_from_numbered_unit_variant(
            "Stetmann2",
            &known_commander_names
        ),
        Some("Stetmann2".to_string())
    );

    for unrelated_unit in [
        "Stetmann",
        "SCV222",
        "CoopCasterStukov",
        "DehakaMutaliskLevel32",
    ] {
        assert_eq!(
            DetailedReplayAnalyzer::commander_name_from_numbered_unit_variant(
                unrelated_unit,
                &known_commander_names
            ),
            None,
            "unexpected commander name from {unrelated_unit}"
        );
    }
}
