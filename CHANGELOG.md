# Changelog

All notable changes to this project will be documented here.

## [1.0.0] — 2026-04-30

### Phase 1 — Workspace scaffolding
- Rust workspace with three crates: `smart-geyser-core`, `smart-geyser-providers`, `smart-geyser-service`
- Docker dev environment (`rust:1-slim-bookworm`) with bind-mounted workspace
- Workspace-level clippy and fmt configuration

### Phase 2 — Core domain logic
- `GeyserState`, `PVSystemState` domain models with explicit unit suffixes
- `GeyserProvider` and `PVSystemProvider` trait surfaces
- Heat calculation math: `heat_lead_time_minutes`, `thermal_energy_stored_kwh`
- `EventDetector`: falling-edge temperature state machine for hot-water use events
- `PatternStore`: time-of-day histogram with exponential decay, JSON persistence
- `SharedState` / `SharedEngineState`: async-safe shared state
- `DecisionEngine`: pattern-based preheat scheduling, smart-stop, legionella safety cycle, boost override
- 14-day integration smoke test

### Phase 3 — Opportunity heating type surface
- `OpportunityReason` enum (BatteryFullExporting / BatteryFullPvCoverage / BatteryFullSocOnly)
- `DecisionIntent::Opportunity` variant added; full OpportunityEngine deferred to v2

### Phase 4 — Providers
- `GeyserwalaProvider`: HTTP provider for Geyserwala Connect (GET state, PATCH element control)
- Token-based auth, configurable timeout, wiremock-tested (11 tests)
- PV providers (Sunsynk, Victron, etc.) deferred to v2

### Phase 5 — Service
- `smart-geyser-service` binary: axum 0.8 REST API + scheduler tick loop
- TOML config loading with `SMART_GEYSER_*` environment variable overrides
- Endpoints: `GET /api/status`, `GET /api/pv-state` (404 in v1), `GET /api/opportunity-log` ([] in v1), `POST /api/boost`, `DELETE /api/boost`, `POST /api/setpoint`
- Multi-stage Docker production image (Debian slim, no build toolchain)
- `config.example.toml`, HA add-on manifest

### Phase 6 — Home Assistant integration
- Python custom component: DataUpdateCoordinator polling `/api/status` every 30 s
- 7 sensor entities, 5 binary sensor entities, setpoint number, boost switch
- Config flow (host + port, validated against live service)
- Services: `smart_geyser.boost`, `smart_geyser.set_setpoint`
- Lovelace dashboard YAML
- Test suite (10 tests, `docker compose run --rm ha-test pytest`)

### Phase 7 — Polish
- `CHANGELOG.md`, `hacs.json`, hardware setup guides in `docs/hardware/`
- `smart-geyser-core` prepared for crates.io publishing

---

## Upcoming (v2)

- `OpportunityEngine`: PV surplus heating (battery-full detection, Paths A/B/C)
- Sunsynk, Victron, GenericHA, GenericMQTT PV providers
- Load-shedding integration (EskomSePush API)
- Energy tracking (solar thermal kWh, PV kWh, grid kWh)
- Options flow for scan interval and opportunity max temp
