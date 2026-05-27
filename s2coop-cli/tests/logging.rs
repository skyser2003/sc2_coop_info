use s2coop_cli::CliLoggingOps;

#[test]
fn display_timestamp_uses_hh_mm_ss_for_console_logs() {
    assert_eq!(CliLoggingOps::format_display_time(0, 0, 0), "00:00:00");
    assert_eq!(CliLoggingOps::format_display_time(23, 4, 5), "23:04:05");
}
