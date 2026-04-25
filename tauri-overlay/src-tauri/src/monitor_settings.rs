use display_info::DisplayInfo;
use tauri::{Manager, Runtime};

use crate::shared_types::MonitorOption;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MonitorDescriptor {
    name: String,
    position_x: i32,
    position_y: i32,
    width: u32,
    height: u32,
}

impl MonitorDescriptor {
    pub fn new(
        name: impl Into<String>,
        position_x: i32,
        position_y: i32,
        width: u32,
        height: u32,
    ) -> Self {
        Self {
            name: name.into(),
            position_x,
            position_y,
            width: width.max(1),
            height: height.max(1),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn position_x(&self) -> i32 {
        self.position_x
    }

    pub fn position_y(&self) -> i32 {
        self.position_y
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }
}

pub struct MonitorSettingsOps;

impl MonitorSettingsOps {
    pub fn normalize_monitor_descriptors(
        mut monitors: Vec<MonitorDescriptor>,
    ) -> Vec<MonitorDescriptor> {
        MonitorSettingsOps::sort_monitor_descriptors(&mut monitors);

        for (idx, monitor) in monitors.iter_mut().enumerate() {
            if monitor.name.trim().is_empty() {
                monitor.name = format!("Monitor {}", idx + 1);
            }
        }

        monitors
    }
}

impl MonitorSettingsOps {
    fn sort_monitor_descriptors(monitors: &mut [MonitorDescriptor]) {
        monitors.sort_by(|left, right| {
            left.position_x
                .cmp(&right.position_x)
                .then(left.position_y.cmp(&right.position_y))
                .then(left.name.cmp(&right.name))
        });
    }
}

impl MonitorSettingsOps {
    fn runtime_monitor_descriptors<R: Runtime>(
        window: &tauri::WebviewWindow<R>,
    ) -> Vec<MonitorDescriptor> {
        let mut monitors = window
            .available_monitors()
            .unwrap_or_default()
            .into_iter()
            .map(|monitor| {
                let position = monitor.position();
                let size = monitor.size();
                MonitorDescriptor::new(
                    monitor
                        .name()
                        .map(|value| value.trim().to_string())
                        .unwrap_or_default(),
                    position.x,
                    position.y,
                    size.width,
                    size.height,
                )
            })
            .collect::<Vec<_>>();

        MonitorSettingsOps::sort_monitor_descriptors(&mut monitors);
        monitors
    }
}

impl MonitorSettingsOps {
    fn named_monitor_descriptors() -> Vec<MonitorDescriptor> {
        let mut monitors = DisplayInfo::all()
            .unwrap_or_default()
            .into_iter()
            .map(|monitor| {
                let name = if !monitor.friendly_name.trim().is_empty() {
                    monitor.friendly_name.trim().to_string()
                } else {
                    monitor.name.trim().to_string()
                };
                MonitorDescriptor::new(name, monitor.x, monitor.y, monitor.width, monitor.height)
            })
            .collect::<Vec<_>>();

        MonitorSettingsOps::sort_monitor_descriptors(&mut monitors);
        monitors
    }
}

impl MonitorSettingsOps {
    pub fn resolve_monitor_descriptors(
        mut runtime_monitors: Vec<MonitorDescriptor>,
        mut named_monitors: Vec<MonitorDescriptor>,
    ) -> Vec<MonitorDescriptor> {
        MonitorSettingsOps::sort_monitor_descriptors(&mut runtime_monitors);
        MonitorSettingsOps::sort_monitor_descriptors(&mut named_monitors);

        if runtime_monitors.is_empty() {
            return MonitorSettingsOps::normalize_monitor_descriptors(named_monitors);
        }
        if named_monitors.is_empty() {
            return MonitorSettingsOps::normalize_monitor_descriptors(runtime_monitors);
        }

        let mut runtime_named = vec![false; runtime_monitors.len()];
        let mut named_used = vec![false; named_monitors.len()];

        for (runtime_index, runtime_monitor) in runtime_monitors.iter_mut().enumerate() {
            let matched = named_monitors
                .iter()
                .enumerate()
                .find(|(named_index, named_monitor)| {
                    !named_used[*named_index]
                        && runtime_monitor.position_x == named_monitor.position_x
                        && runtime_monitor.position_y == named_monitor.position_y
                });
            let Some((named_index, named_monitor)) = matched else {
                continue;
            };
            if !named_monitor.name.trim().is_empty() {
                runtime_monitor.name = named_monitor.name.clone();
                runtime_named[runtime_index] = true;
            }
            named_used[named_index] = true;
        }

        if runtime_monitors.len() == named_monitors.len() {
            for (runtime_index, runtime_monitor) in runtime_monitors.iter_mut().enumerate() {
                if runtime_named[runtime_index] {
                    continue;
                }
                let matched =
                    named_monitors
                        .iter()
                        .enumerate()
                        .find(|(named_index, named_monitor)| {
                            !named_used[*named_index] && !named_monitor.name.trim().is_empty()
                        });
                let Some((named_index, named_monitor)) = matched else {
                    continue;
                };
                runtime_monitor.name = named_monitor.name.clone();
                named_used[named_index] = true;
            }
        }

        MonitorSettingsOps::normalize_monitor_descriptors(runtime_monitors)
    }
}

impl MonitorSettingsOps {
    pub fn monitor_descriptors<R: Runtime>(
        window: &tauri::WebviewWindow<R>,
    ) -> Vec<MonitorDescriptor> {
        MonitorSettingsOps::resolve_monitor_descriptors(
            MonitorSettingsOps::runtime_monitor_descriptors(window),
            MonitorSettingsOps::named_monitor_descriptors(),
        )
    }
}

impl MonitorSettingsOps {
    pub fn selected_monitor_index(requested_monitor: usize, monitor_count: usize) -> Option<usize> {
        if monitor_count == 0 {
            return None;
        }

        Some(
            requested_monitor
                .max(1)
                .saturating_sub(1)
                .min(monitor_count - 1),
        )
    }
}

impl MonitorSettingsOps {
    pub fn selected_monitor_descriptor(
        monitors: &[MonitorDescriptor],
        requested_monitor: usize,
    ) -> Option<&MonitorDescriptor> {
        let index = MonitorSettingsOps::selected_monitor_index(requested_monitor, monitors.len())?;
        monitors.get(index)
    }
}

impl MonitorSettingsOps {
    pub fn selected_monitor_for_window<R: Runtime>(
        window: &tauri::WebviewWindow<R>,
        requested_monitor: usize,
    ) -> Result<MonitorDescriptor, String> {
        let monitors = MonitorSettingsOps::monitor_descriptors(window);
        MonitorSettingsOps::selected_monitor_descriptor(&monitors, requested_monitor)
            .cloned()
            .ok_or_else(|| "No monitors detected".to_string())
    }
}

impl MonitorSettingsOps {
    pub fn monitor_catalog_from_descriptors(monitors: &[MonitorDescriptor]) -> Vec<MonitorOption> {
        monitors
            .iter()
            .enumerate()
            .map(|(idx, monitor)| MonitorOption {
                index: idx + 1,
                label: format!("{} - {}", idx + 1, monitor.name()),
            })
            .collect()
    }
}

impl MonitorSettingsOps {
    pub fn available_monitor_catalog<R: Runtime>(app: &tauri::AppHandle<R>) -> Vec<MonitorOption> {
        let window = app
            .get_webview_window("config")
            .or_else(|| app.get_webview_window("overlay"))
            .or_else(|| app.get_webview_window("performance"));
        let Some(window) = window else {
            return Vec::new();
        };

        MonitorSettingsOps::monitor_catalog_from_descriptors(
            &MonitorSettingsOps::monitor_descriptors(&window),
        )
    }
}
