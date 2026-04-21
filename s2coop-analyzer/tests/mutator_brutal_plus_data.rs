mod common;

#[test]
fn brutal_plus_mutator_ranges_match_expected_budgets() {
    let dictionary = common::load_dictionary();
    let entries = &dictionary.mutator_brutal_plus;

    assert_eq!(entries.len(), 6, "expected Brutal+ levels 1 through 6");

    let expected = [
        (1_u8, 4_u32, 6_u32, 2_u32, 3_u32),
        (2_u8, 7_u32, 8_u32, 2_u32, 3_u32),
        (3_u8, 9_u32, 10_u32, 2_u32, 3_u32),
        (4_u8, 11_u32, 12_u32, 2_u32, 3_u32),
        (5_u8, 15_u32, 16_u32, 2_u32, 4_u32),
        (6_u8, 19_u32, 20_u32, 2_u32, 4_u32),
    ];

    for (entry, expected_entry) in entries.iter().zip(expected) {
        assert_eq!(entry.brutal_plus, expected_entry.0);
        assert_eq!(entry.mutator_points.min, expected_entry.1);
        assert_eq!(entry.mutator_points.max, expected_entry.2);
        assert_eq!(entry.mutator_count.min, expected_entry.3);
        assert_eq!(entry.mutator_count.max, expected_entry.4);
    }
}
