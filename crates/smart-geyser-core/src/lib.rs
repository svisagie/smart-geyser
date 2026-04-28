//! `smart-geyser-core` — pure, hardware-agnostic logic for the smart-geyser
//! controller.
//!
//! This crate intentionally has **no I/O and no async runtime concerns**.
//! It defines:
//!
//! * Domain models (`models`) — `GeyserState`, `PVSystemState`,
//!   `OpportunityConfig`, `EngineConfig`, etc.
//! * The heating-system taxonomy (`system`) — `HeatingSystem`, `PumpVoltage`.
//! * Provider traits (`provider`, `pv_provider`) — `GeyserProvider`,
//!   `PVSystemProvider` and their capability enums.
//! * Heat-calculation math (`heat_calc`) — pure functions, no I/O.
//!
//! The architectural rationale lives in the project spec
//! (`smart-geyser-spec-v5_1.md`, §1).
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod decision_engine;
pub mod event_detector;
pub mod heat_calc;
pub mod models;
pub mod pattern_store;
pub mod provider;
pub mod pv_provider;
pub mod shared_state;
pub mod system;

// Re-export the public surface so callers can write
// `use smart_geyser_core::GeyserProvider` instead of the full module path.
pub use heat_calc::{energy_to_heat_kwh, heat_lead_time_minutes, thermal_energy_stored_kwh};
pub use models::{
    EngineConfig, GeyserState, OpportunityConfig, PVCapabilities, PVCapability, PVSystemState,
    SolarWindow,
};
pub use provider::{GeyserCapabilities, GeyserCapability, GeyserProvider};
pub use pv_provider::PVSystemProvider;
pub use system::{HeatingSystem, PumpVoltage};
