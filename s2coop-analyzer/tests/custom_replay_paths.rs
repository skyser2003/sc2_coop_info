use s2coop_analyzer::detailed_replay_analysis::DetailedReplayAnalyzer;
use std::path::Path;

#[test]
fn games_tab_custom_replay_path_matches_mm_and_normalized_coop_file_names() {
    let included_paths = [
        "Accounts/1-S2-1/Replays/[MM] Lock And Load.SC2Replay",
        "Accounts/1-S2-1/Replays/Nexus Co-op Dead of Night.SC2Replay",
        "Accounts/1-S2-1/Replays/Co-op+.SC2Replay",
        "Accounts/1-S2-1/Replays/C.O_O P.SC2Replay",
        "Accounts/1-S2-1/Replays/COOPERATIVE Mission.SC2Replay",
    ];

    for path in included_paths {
        assert!(
            DetailedReplayAnalyzer::is_games_tab_custom_replay_path(Path::new(path)),
            "expected transient custom replay path: {path}"
        );
    }
}

#[test]
fn games_tab_custom_replay_path_ignores_unrelated_file_names_and_parent_folders() {
    let excluded_paths = [
        "Accounts/1-S2-1/Replays/Void Launch.SC2Replay",
        "Accounts/1-S2-1/Replays/[mm] Lock And Load.SC2Replay",
        "Accounts/Co-op/Replays/Dead of Night.SC2Replay",
        "Accounts/1-S2-1/Replays/Coup Mission.SC2Replay",
    ];

    for path in excluded_paths {
        assert!(
            !DetailedReplayAnalyzer::is_games_tab_custom_replay_path(Path::new(path)),
            "expected standard replay path: {path}"
        );
    }
}

#[test]
fn nexus_coop_replay_path_is_identified_after_normalization() {
    assert!(DetailedReplayAnalyzer::is_nexus_coop_replay_path(
        Path::new("Replays/Nexus Co-op Dead of Night.SC2Replay")
    ));
    assert!(!DetailedReplayAnalyzer::is_nexus_coop_replay_path(
        Path::new("Replays/[Co-op+] Part and Parcel.SC2Replay")
    ));
}

#[test]
fn coop_plus_replay_path_is_identified_without_matching_other_coop_maps() {
    for path in [
        "Replays/[Co-op+] Part and Parcel.SC2Replay",
        "Replays/Coop Plus Part and Parcel.SC2Replay",
    ] {
        assert!(
            DetailedReplayAnalyzer::is_coop_plus_replay_path(Path::new(path)),
            "expected Co-op+ replay path: {path}"
        );
    }

    for path in [
        "Replays/Nexus Co-op Dead of Night.SC2Replay",
        "Replays/Cooperative Mission.SC2Replay",
    ] {
        assert!(
            !DetailedReplayAnalyzer::is_coop_plus_replay_path(Path::new(path)),
            "unexpected Co-op+ replay path: {path}"
        );
    }
}
