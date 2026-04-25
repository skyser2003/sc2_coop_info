use sco_tauri_overlay::monitor_settings::{MonitorDescriptor, MonitorSettingsOps};

#[test]
fn resolved_monitor_descriptors_keep_runtime_geometry_for_monitor_one_alignment() {
    let runtime_monitors = vec![
        MonitorDescriptor::new(r"\\.\DISPLAY1", -1080, 0, 1080, 1920),
        MonitorDescriptor::new(r"\\.\DISPLAY2", 0, 0, 2560, 1440),
    ];
    let named_monitors = vec![
        MonitorDescriptor::new("Portrait", -1080, 0, 1440, 2560),
        MonitorDescriptor::new("Primary", 0, 0, 2560, 1440),
    ];

    let resolved =
        MonitorSettingsOps::resolve_monitor_descriptors(runtime_monitors, named_monitors);

    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved[0].name(), "Portrait");
    assert_eq!(resolved[0].position_x(), -1080);
    assert_eq!(resolved[0].width(), 1080);
    assert_eq!(resolved[0].height(), 1920);
    assert_eq!(resolved[1].name(), "Primary");
    assert_eq!(resolved[1].position_x(), 0);
    assert_eq!(resolved[1].width(), 2560);
}

#[test]
fn resolved_monitor_descriptors_fall_back_to_display_info_when_runtime_is_missing() {
    let named_monitors = vec![
        MonitorDescriptor::new("Portrait", -1080, 0, 1080, 1920),
        MonitorDescriptor::new("Primary", 0, 0, 2560, 1440),
    ];

    let resolved = MonitorSettingsOps::resolve_monitor_descriptors(Vec::new(), named_monitors);

    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved[0].name(), "Portrait");
    assert_eq!(resolved[1].name(), "Primary");
}
