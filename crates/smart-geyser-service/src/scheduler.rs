//! The tick loop — polls hardware, runs engines, applies element control.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use smart_geyser_core::decision_engine::{DecisionEngine, DecisionIntent};
use smart_geyser_core::event_detector::EventDetector;
use smart_geyser_core::provider::{GeyserCapability, GeyserProvider};
use smart_geyser_core::system::HeatingSystem;

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
    /// Read the device setpoint and adopt it when the device itself changed it.
    /// Returns without writing to the Arc if the change came from our own push.
    async fn sync_device_setpoint(&self, last_pushed: &mut f32, last_known: &mut f32) {
        match self.geyser.get_setpoint().await {
            Ok(Some(device_sp)) => {
                if (device_sp - *last_known).abs() > 0.5 {
                    info!(
                        device_sp,
                        prev = *last_known,
                        "device setpoint changed externally — adopting"
                    );
                    *self.setpoint_c.write().await = device_sp;
                    *last_pushed = device_sp;
                    *last_known = device_sp;
                }
            }
            Ok(None) => {}
            Err(e) => warn!("get_setpoint failed: {e:#}"),
        }
    }

    /// Serialise current state and broadcast to SSE subscribers.
    async fn broadcast_status(&self, sp: f32) {
        let shared = self.app_state.shared.read().await;
        let snap = self.app_state.snapshot.read().await;
        let system_type = match self.geyser.system() {
            HeatingSystem::ElectricOnly => "electric_only",
            HeatingSystem::SolarPumped => "solar_pumped",
            HeatingSystem::HeatPump { .. } => "heat_pump",
        };
        let event = serde_json::json!({
            "system_type": system_type,
            "provider": self.app_state.provider.geyser_name,
            "setpoint_c": sp,
            "tank_temp_c": snap.geyser.as_ref().map(|g| g.tank_temp_c),
            "collector_temp_c": snap.geyser.as_ref().and_then(|g| g.collector_temp_c),
            "pump_active": snap.geyser.as_ref().and_then(|g| g.pump_active),
            "heating_active": snap.geyser.as_ref().map(|g| g.heating_active),
            "smart_stop_active": shared.smart_stop_active,
            "preheat_active": shared.preheat_active,
            "read_only_mode": shared.read_only_mode,
            "boost_until": shared.boost_until,
            "next_predicted_use": snap.next_predicted_use,
            "preheat_starts_at": snap.preheat_starts_at,
            "events_today": 0_u32,
        });
        if let Ok(json) = serde_json::to_string(&event) {
            *self.app_state.last_status_event.write().await = Some(json.clone());
            let _ = self.app_state.events_tx.send(json);
        }
    }

    async fn apply_element_control(
        &self,
        intent: &DecisionIntent,
        has_native_boost: bool,
        last_boost_active: &mut Option<bool>,
        last_element_on: &mut Option<bool>,
    ) {
        let is_boost = matches!(intent, DecisionIntent::Boost { .. });
        let element_on = matches!(
            intent,
            DecisionIntent::Preheat { .. }
                | DecisionIntent::Boost { .. }
                | DecisionIntent::Opportunity { .. }
        );
        let read_only = self.app_state.shared.read().await.read_only_mode;

        if has_native_boost {
            if *last_boost_active != Some(is_boost) {
                if read_only {
                    info!(
                        ?intent,
                        is_boost, "read-only — would set boost-demand to {is_boost}"
                    );
                } else {
                    info!(?intent, is_boost, "native boost state changed");
                    if let Err(e) = self.geyser.set_boost(is_boost).await {
                        warn!("set_boost({is_boost}) failed: {e:#}");
                    }
                    if !is_boost {
                        *last_element_on = None;
                    }
                }
            }
            *last_boost_active = Some(is_boost);

            if !is_boost {
                if read_only {
                    if *last_element_on != Some(element_on) {
                        info!(
                            ?intent,
                            element_on, "read-only — would set element to {element_on}"
                        );
                    }
                } else if *last_element_on != Some(element_on) {
                    info!(?intent, element_on, "element state changed");
                    if let Err(e) = self.geyser.set_element(element_on).await {
                        warn!("set_element({element_on}) failed: {e:#}");
                    }
                }
                *last_element_on = Some(element_on);
            }
        } else {
            if read_only {
                if *last_element_on != Some(element_on) {
                    info!(
                        ?intent,
                        element_on, "read-only — would set element to {element_on}"
                    );
                }
            } else if *last_element_on != Some(element_on) {
                info!(?intent, element_on, "element state changed");
                if let Err(e) = self.geyser.set_element(element_on).await {
                    warn!("set_element({element_on}) failed: {e:#}");
                }
            }
            *last_element_on = Some(element_on);
        }
    }

    /// Run the tick loop indefinitely.
    pub async fn run(mut self, tick_interval: Duration) {
        let caps = self.geyser.capabilities();
        let has_native_boost = caps.contains(&GeyserCapability::BoostControl);
        let has_setpoint_control = caps.contains(&GeyserCapability::SetpointControl);
        let mut last_decay_date: Option<chrono::NaiveDate> = None;
        let mut last_element_on: Option<bool> = None;
        let mut last_boost_active: Option<bool> = None;
        let initial_sp = *self.setpoint_c.read().await;
        let mut last_pushed_setpoint: f32 = initial_sp;
        let mut last_known_device_setpoint: f32 = initial_sp;

        loop {
            let now = Utc::now();

            // Adopt any setpoint change made directly on the device.
            if has_setpoint_control {
                self.sync_device_setpoint(
                    &mut last_pushed_setpoint,
                    &mut last_known_device_setpoint,
                )
                .await;
            }

            // Read current setpoint (authoritative after device sync above).
            let sp = *self.setpoint_c.read().await;
            self.engine.set_setpoint(sp);

            // Push API-driven setpoint change to device.
            if has_setpoint_control && (sp - last_pushed_setpoint).abs() > f32::EPSILON {
                info!(
                    setpoint_c = sp,
                    "setpoint changed via API — pushing to device"
                );
                if let Err(e) = self.geyser.set_setpoint(sp).await {
                    warn!("set_setpoint({sp}) failed: {e:#}");
                } else {
                    last_pushed_setpoint = sp;
                    last_known_device_setpoint = sp;
                }
            }

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

            // Apply element / boost control.
            self.apply_element_control(
                &intent,
                has_native_boost,
                &mut last_boost_active,
                &mut last_element_on,
            )
            .await;

            // Update the shared snapshot for the API and SSE broadcast.
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
            self.broadcast_status(sp).await;

            // Wait for either the interval or an event-driven wake (MQTT message, API call).
            tokio::select! {
                () = tokio::time::sleep(tick_interval) => {}
                () = self.app_state.tick_notify.notified() => {
                    debug!("tick triggered by event");
                }
            }
        }
    }
}
