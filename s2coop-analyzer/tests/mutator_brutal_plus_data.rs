use s2coop_analyzer::dictionary_data;

#[test]
fn brutal_plus_mutator_ranges_match_expected_budgets() {
    let entries = dictionary_data::mutator_brutal_plus();

    assert_eq!(entries.len(), 6, "expected Brutal+ levels 1 through 6");

    let expected = [
        (1_u8, 4_u64, 6_u64, 2_u64, 3_u64),
        (2_u8, 7_u64, 8_u64, 2_u64, 3_u64),
        (3_u8, 9_u64, 10_u64, 2_u64, 3_u64),
        (4_u8, 11_u64, 12_u64, 2_u64, 3_u64),
        (5_u8, 15_u64, 16_u64, 2_u64, 4_u64),
        (6_u8, 19_u64, 20_u64, 2_u64, 4_u64),
    ];

    for (entry, expected_entry) in entries.iter().zip(expected) {
        assert_eq!(entry.brutal_plus, expected_entry.0);
        assert_eq!(entry.mutator_points.min, expected_entry.1);
        assert_eq!(entry.mutator_points.max, expected_entry.2);
        assert_eq!(entry.mutator_count.min, expected_entry.3);
        assert_eq!(entry.mutator_count.max, expected_entry.4);
    }
}
