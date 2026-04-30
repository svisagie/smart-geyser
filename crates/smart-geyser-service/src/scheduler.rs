//! The tick loop — polls hardware, runs engines, applies element control.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use smart_geyser_core::decision_engine::{DecisionEngine, DecisionIntent};
use smart_geyser_core::event_detector::EventDetector;
use smart_geyser_core::provider::GeyserProvider;

use crate::app_state::AppState;

pub struct Scheduler {
    pub geyser: Box<dyn GeyserProvider>,
    pub engine: DecisionEngine,
    pub detector: EventDetector,
    pub app_state: AppState,
    pub data_dir: PathBuf,
    /// Shared setpoint so the API can update it between ticks.
    pub setpoint_c: Arc<RwLock<f32>>,
}

impl Scheduler {
    /// Run the tick loop indefinitely.
    pub async fn run(mut self, tick_interval: Duration) {
        let mut last_decay_date: Option<chrono::NaiveDate> = None;
        let mut last_element_on: Option<bool> = None;

        loop {
            let now = Utc::now();

            // Sync setpoint from the shared value (API may have updated it).
            let sp = *self.setpoint_c.read().await;
            self.engine.set_setpoint(sp);

            // Apply daily decay once per calendar day.
            let today = now.date_naive();
            if last_decay_date != Some(today) {
                self.engine.apply_daily_decay(today);
                last_decay_date = Some(today);
            }

            // Poll geyser state.
            let geyser_state = match self.geyser.get_state().await {
                Ok(s) => s,
                Err(e) => {
                    warn!("geyser poll failed, skipping tick: {e:#}");
                    tokio::time::sleep(tick_interval).await;
                    continue;
                }
            };

            // Feed state through event detector; record any use event.
            if let Some(event) = self.detector.feed(geyser_state.clone()) {
                info!(
                    temp_drop_c = event.temp_drop_c,
                    estimated_volume_l = event.estimated_volume_l,
                    confidence = event.confidence,
                    "hot-water use event detected"
                );
                self.engine.record_event(&event);

                // Persist pattern store after each new event.
                if !self.data_dir.as_os_str().is_empty() {
                    let path = self.data_dir.join("pattern_store.json");
                    if let Err(e) = self.engine.save_pattern_store(&path) {
                        warn!("failed to persist pattern store: {e:#}");
                    }
                }
            }

            // Run decision engine.
            let intent = self.engine.tick(&geyser_state, now).await;
            debug!(?intent, tank_temp_c = geyser_state.tank_temp_c, "tick");

            // Apply element control (skipped in read-only mode).
            let element_on = matches!(
                intent,
                DecisionIntent::Preheat { .. }
                    | DecisionIntent::Boost { .. }
                    | DecisionIntent::Opportunity { .. }
            );
            let read_only = self.app_state.shared.read().await.read_only_mode;
            if read_only {
                if last_element_on != Some(element_on) {
                    info!(
                        ?intent,
                        element_on,
                        "read-only mode — would set element to {element_on} but skipping"
                    );
                }
            } else {
                if last_element_on != Some(element_on) {
                    info!(?intent, element_on, "element state changed");
                }
                if let Err(e) = self.geyser.set_element(element_on).await {
                    warn!("set_element({element_on}) failed: {e:#}");
                }
            }
            last_element_on = Some(element_on);

            // Update the shared snapshot for the API.
            let next_use = self.engine.next_use_window(now);
            let preheat_starts_at = next_use.map(|t| {
                use smart_geyser_core::heat_calc::heat_lead_time_minutes;
                let lead = heat_lead_time_minutes(&geyser_state, sp, &self.geyser.system());
                t - chrono::Duration::minutes(i64::from(lead))
            });

            {
                let mut snap = self.app_state.snapshot.write().await;
                snap.geyser = Some(geyser_state);
                snap.next_predicted_use = next_use;
                snap.preheat_starts_at = preheat_starts_at;
            }

            tokio::time::sleep(tick_interval).await;
        }
    }
}
