use sco_tauri_overlay::monitor_settings::{
    monitor_catalog_from_descriptors, normalize_monitor_descriptors, resolve_monitor_descriptors,
    selected_monitor_descriptor, selected_monitor_index, MonitorDescriptor,
};

#[test]
fn normalize_monitor_descriptors_sorts_by_geometry_and_fills_empty_names() {
    let monitors = vec![
        MonitorDescriptor::new("", 1920, 0, 2560, 1440),
        MonitorDescriptor::new("Portrait", -1080, 0, 1080, 1920),
        MonitorDescriptor::new("Primary", 0, 0, 1920, 1080),
    ];

    let normalized = normalize_monitor_descriptors(monitors);

    assert_eq!(normalized[0].name(), "Portrait");
    assert_eq!(normalized[0].position_x(), -1080);
    assert_eq!(normalized[1].name(), "Primary");
    assert_eq!(normalized[1].position_x(), 0);
    assert_eq!(normalized[2].name(), "Monitor 3");
    assert_eq!(normalized[2].position_x(), 1920);
}

#[test]
fn selected_monitor_descriptor_clamps_to_last_available_monitor() {
    let monitors = normalize_monitor_descriptors(vec![
        MonitorDescriptor::new("Left", -1080, 0, 1080, 1920),
        MonitorDescriptor::new("Center", 0, 0, 2560, 1440),
    ]);

    assert_eq!(selected_monitor_index(1, monitors.len()), Some(0));
    assert_eq!(selected_monitor_index(2, monitors.len()), Some(1));
    assert_eq!(selected_monitor_index(3, monitors.len()), Some(1));
    assert_eq!(selected_monitor_index(0, monitors.len()), Some(0));

    let selected = selected_monitor_descriptor(&monitors, 3).expect("monitor should exist");
    assert_eq!(selected.name(), "Center");
}

#[test]
fn monitor_catalog_from_descriptors_uses_one_based_labels() {
    let monitors = normalize_monitor_descriptors(vec![
        MonitorDescriptor::new("Left", -1080, 0, 1080, 1920),
        MonitorDescriptor::new("Primary", 0, 0, 2560, 1440),
    ]);

    let catalog = monitor_catalog_from_descriptors(&monitors);

    assert_eq!(catalog.len(), 2);
    assert_eq!(catalog[0].index, 1);
    assert_eq!(catalog[0].label, "1 - Left");
    assert_eq!(catalog[1].index, 2);
    assert_eq!(catalog[1].label, "2 - Primary");
}

#[test]
fn resolve_monitor_descriptors_prefers_friendly_names_when_positions_match() {
    let runtime_monitors = vec![
        MonitorDescriptor::new(r"\\.\DISPLAY2", 0, 0, 2560, 1440),
        MonitorDescriptor::new(r"\\.\DISPLAY1", -1080, 0, 1080, 1920),
    ];
    let named_monitors = vec![
        MonitorDescriptor::new("Primary", 0, 0, 2560, 1440),
        MonitorDescriptor::new("Portrait", -1080, 0, 1080, 1920),
    ];

    let resolved = resolve_monitor_descriptors(runtime_monitors, named_monitors);

    assert_eq!(resolved[0].name(), "Portrait");
    assert_eq!(resolved[1].name(), "Primary");
}
