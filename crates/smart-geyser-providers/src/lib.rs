//! Concrete implementations of the `smart-geyser-core` provider traits.
//!
//! Only the geyser-side providers are implemented here for v1. PV providers
//! (`SunsynkProvider`, `VictronProvider`, etc.) are deferred to v2; pass
//! `None` for the PV provider in the service to run without PV integration.

pub mod geyserwala;
pub mod geyserwala_mqtt;
