# smart-geyser-core

Hardware-agnostic core logic for the [smart-geyser](https://github.com/svisagie/smart-geyser) controller.

## What's in this crate

- **`GeyserProvider` / `PVSystemProvider` traits** — implement these for any geyser hardware or PV inverter
- **Domain models** — `GeyserState`, `PVSystemState`, `EngineConfig`, `HeatingSystem`, etc.
- **Heat-calculation math** — `heat_lead_time_minutes`, `thermal_energy_stored_kwh`
- **`DecisionEngine`** — pattern-based preheat scheduling, smart-stop, legionella safety
- **`EventDetector`** — falling-edge temperature state machine for hot-water use events
- **`PatternStore`** — time-of-day histogram with exponential decay and JSON persistence

## Quick example

```rust
use smart_geyser_core::{GeyserProvider, EngineConfig};
use smart_geyser_core::decision_engine::DecisionEngine;
use smart_geyser_core::pattern_store::PatternStore;
use smart_geyser_core::shared_state::SharedState;

let config = EngineConfig::default();
let store = PatternStore::new(config.decay_factor);
let shared = SharedState::new();
let mut engine = DecisionEngine::new(config, store, shared);

// In your tick loop:
// let intent = engine.tick(&geyser_state, chrono::Utc::now()).await;
```

## Links

- [Full documentation](https://docs.rs/smart-geyser-core)
- [Project repository](https://github.com/svisagie/smart-geyser)
- [Service crate](https://github.com/svisagie/smart-geyser/tree/main/crates/smart-geyser-service)
