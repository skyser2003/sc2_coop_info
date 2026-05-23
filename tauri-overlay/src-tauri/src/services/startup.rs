use std::thread;
use std::time::Instant;
use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_updater::UpdaterExt;

use crate::{BackendState, TauriOverlayOps};

impl TauriOverlayOps {
    pub fn spawn_protocol_store_warmup() {
        thread::spawn(|| {
            let started_at = Instant::now();
            match s2protocol_port::ProtocolStoreBuilder::build() {
                Ok(_) => {
                    crate::sco_log!(
                        "[SCO/protocol] warmup completed in {}ms",
                        started_at.elapsed().as_millis()
                    );
                }
                Err(error) => {
                    crate::sco_log!("[SCO/protocol] warmup failed: {error}");
                }
            }
        });
    }

    pub fn spawn_replay_analysis_resource_warmup(app: AppHandle<Wry>) {
        thread::spawn(move || {
            let started_at = Instant::now();
            let state = app.state::<BackendState>();
            match state.replay_analysis_resources() {
                Ok(_) => {
                    crate::sco_log!(
                        "[SCO/analyzer] warmup completed in {}ms",
                        started_at.elapsed().as_millis()
                    );
                }
                Err(error) => {
                    crate::sco_log!("[SCO/analyzer] warmup failed: {error}");
                }
            }
        });
    }

    pub async fn auto_update(handle: tauri::AppHandle) -> tauri_plugin_updater::Result<()> {
        if let Some(update) = handle.updater()?.check().await? {
            crate::sco_log!("Auto update begin");

            let mut downloaded = 0;

            // alternatively we could also call update.download() and update.install() separately
            update
                .download_and_install(
                    |chunk_length, content_length| {
                        downloaded += chunk_length;
                        crate::sco_log!("downloaded {downloaded} from {content_length:?}");
                    },
                    || {
                        crate::sco_log!("download finished");
                    },
                )
                .await?;

            crate::sco_log!("update installed");
            handle.restart();
        }

        Ok(())
    }
}
