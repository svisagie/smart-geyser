//! Shared mutable state threaded between the decision and opportunity engines.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Default)]
pub struct SharedEngineState {
    pub smart_stop_active: bool,
    pub opportunity_active: bool,
    pub last_opportunity_start: Option<DateTime<Utc>>,
    pub preheat_active: bool,
    pub boost_until: Option<DateTime<Utc>>,
    pub last_high_temp_event: Option<DateTime<Utc>>,
    /// When true the scheduler observes and reports but never calls set_element.
    pub read_only_mode: bool,
}

#[derive(Clone)]
pub struct SharedState(Arc<RwLock<SharedEngineState>>);

impl Default for SharedState {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedState {
    #[must_use]
    pub fn new() -> Self {
        Self(Arc::new(RwLock::new(SharedEngineState::default())))
    }

    pub async fn read(&self) -> tokio::sync::RwLockReadGuard<'_, SharedEngineState> {
        self.0.read().await
    }

    pub async fn write(&self) -> tokio::sync::RwLockWriteGuard<'_, SharedEngineState> {
        self.0.write().await
    }

    pub async fn set_preheat(&self, active: bool) {
        self.0.write().await.preheat_active = active;
    }

    pub async fn set_smart_stop(&self, active: bool) {
        self.0.write().await.smart_stop_active = active;
    }

    pub async fn set_opportunity(&self, active: bool, now: DateTime<Utc>) {
        let mut state = self.0.write().await;
        state.opportunity_active = active;
        state.last_opportunity_start = Some(now);
    }

    pub async fn set_boost_until(&self, until: Option<DateTime<Utc>>) {
        self.0.write().await.boost_until = until;
    }

    pub async fn record_high_temp_event(&self, at: DateTime<Utc>) {
        self.0.write().await.last_high_temp_event = Some(at);
    }

    pub async fn set_read_only(&self, active: bool) {
        self.0.write().await.read_only_mode = active;
    }
}

#[must_use]
pub fn is_boosting(state: &SharedEngineState, now: DateTime<Utc>) -> bool {
    state.boost_until.is_some_and(|t| t > now)
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn default_state_is_all_inactive() {
        let state = SharedEngineState::default();
        assert_eq!(state.smart_stop_active, false);
        assert_eq!(state.opportunity_active, false);
        assert_eq!(state.last_opportunity_start, None);
        assert_eq!(state.preheat_active, false);
        assert_eq!(state.boost_until, None);
        assert_eq!(state.last_high_temp_event, None);
    }

    #[tokio::test]
    async fn set_preheat_roundtrip() {
        let shared = SharedState::new();

        shared.set_preheat(true).await;
        assert_eq!(shared.read().await.preheat_active, true);

        shared.set_preheat(false).await;
        assert_eq!(shared.read().await.preheat_active, false);
    }

    #[tokio::test]
    async fn set_smart_stop_roundtrip() {
        let shared = SharedState::new();

        shared.set_smart_stop(true).await;
        assert_eq!(shared.read().await.smart_stop_active, true);

        shared.set_smart_stop(false).await;
        assert_eq!(shared.read().await.smart_stop_active, false);
    }

    #[tokio::test]
    async fn set_opportunity_records_timestamp() {
        let shared = SharedState::new();
        let now = Utc.with_ymd_and_hms(2026, 4, 28, 10, 0, 0).unwrap();

        shared.set_opportunity(true, now).await;

        let state = shared.read().await;
        assert_eq!(state.opportunity_active, true);
        assert_eq!(state.last_opportunity_start, Some(now));
    }

    #[tokio::test]
    async fn set_boost_until_and_is_boosting() {
        let shared = SharedState::new();
        let now = Utc.with_ymd_and_hms(2026, 4, 28, 10, 0, 0).unwrap();
        let ten_min_later = now + chrono::Duration::minutes(10);
        let eleven_min_later = now + chrono::Duration::minutes(11);

        shared.set_boost_until(Some(ten_min_later)).await;

        let state = shared.read().await;
        assert_eq!(is_boosting(&state, now), true);
        assert_eq!(is_boosting(&state, eleven_min_later), false);
    }

    #[tokio::test]
    async fn concurrent_read_write() {
        let shared = SharedState::new();
        let writer = shared.clone();
        let reader = shared.clone();

        let write_task = tokio::spawn(async move {
            for i in 0..100 {
                writer.set_preheat(i % 2 == 0).await;
            }
        });

        let read_task = tokio::spawn(async move {
            for _ in 0..100 {
                let _ = reader.read().await.preheat_active;
            }
        });

        write_task.await.unwrap();
        read_task.await.unwrap();
    }

    #[tokio::test]
    async fn record_high_temp_event_stored() {
        let shared = SharedState::new();
        let now = Utc.with_ymd_and_hms(2026, 4, 28, 12, 30, 0).unwrap();

        shared.record_high_temp_event(now).await;

        assert_eq!(shared.read().await.last_high_temp_event, Some(now));
    }
}
