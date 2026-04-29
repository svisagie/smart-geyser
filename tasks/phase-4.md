# Phase 4 — Providers

**Spec:** [../smart-geyser-spec-v5_1.md](../smart-geyser-spec-v5_1.md) §13 Phase 4, §5 (repo structure), §6 (PV providers), §6.1–6.4 (per-provider config and topic maps)

**Goal:** Implement all concrete provider crates — both geyser-side (`GeyserwalaProvider`, `GenericHAEntityProvider`, `GenericMqttProvider`) and PV-side (`VictronProvider`, `SunsynkProvider`, `GenericHAPVProvider`, `GenericMqttPVProvider`) — so that the engines built in Phases 1–3 can be wired to real hardware. No engine logic changes here; this phase is entirely about I/O adapters that satisfy the traits defined in `smart-geyser-core`.

**Exit criteria:** `cargo test --workspace` passes (all crates). Each provider compiles, satisfies its trait, correctly reports `capabilities()`, and passes against a mock/stub transport. The providers crate is structured per spec §5 and imports `smart-geyser-core` but introduces no logic that belongs in core.

---

## 1. `smart-geyser-providers` crate scaffolding

- [ ] 1.1 Create `crates/smart-geyser-providers/` with `Cargo.toml`; add it to the workspace `members` list
- [ ] 1.2 Add `smart-geyser-core` as a path dependency; add runtime dependencies: `async-trait`, `anyhow`, `thiserror`, `serde`, `serde_json`, `tokio` (full features), `chrono`
- [ ] 1.3 Add MQTT dependency (`rumqttc` or equivalent async client); add HTTP client dependency `reqwest` (with `json` feature) for HA-based providers
- [ ] 1.4 Create `src/lib.rs` with module declarations for `geyserwala`, `ha_entity`, `mqtt_generic`, and `pv/` submodule
- [ ] 1.5 Create `src/pv/mod.rs` with module declarations for `victron`, `sunsynk`, `ha_entity_pv`, `mqtt_generic_pv` per spec §5 folder layout
- [ ] 1.6 Add dev-dependencies: `tokio` (test feature), `wiremock` (for HTTP stubbing), `rstest`, `pretty_assertions`
- [ ] 1.7 Confirm `docker compose run --rm dev cargo build --workspace` succeeds with empty provider stubs

## 2. `GeyserwalaProvider` (geyser provider)

The Geyserwala Connect exposes a local HTTP API. This provider polls it and maps the response to `GeyserState`, and sends element/pump commands via HTTP POST.

- [ ] 2.1 Define `GeyserwalaConfig` struct: `base_url: String`, `poll_interval_secs: u32`, `timeout_secs: u32`
- [ ] 2.2 Define `GeyserwalaProvider` struct holding `GeyserwalaConfig` and a `reqwest::Client`
- [ ] 2.3 Implement `get_state() -> anyhow::Result<GeyserState>`: GET the Geyserwala status endpoint, deserialise the JSON response, map all fields to `GeyserState` (tank_temp_c, collector_temp_c, pump_active, heating_active, element_kw, tank_volume_l, timestamp)
- [ ] 2.4 Implement `set_element(on: bool)`: POST to the Geyserwala element control endpoint; propagate HTTP errors as `anyhow::Error`
- [ ] 2.5 Implement `set_pump(on: bool)`: POST to the pump control endpoint; no-op or `Err` if the hardware doesn't support direct pump control
- [ ] 2.6 Implement `capabilities()`: return the full `GeyserCapability` set — `TankTemp`, `CollectorTemp`, `PumpControl`, `ElementControl`, `BoostControl`, `FaultStatus`
- [ ] 2.7 Implement `name()`: return `"Geyserwala Connect"`
- [ ] 2.8 Implement `system()`: return `HeatingSystem::SolarPumped`
- [ ] 2.9 Map Geyserwala fault/alarm fields to `FaultStatus` — surface any reported fault as an `Err` or a dedicated fault flag in `GeyserState`
- [ ] 2.10 Unit tests with a `wiremock` HTTP server:
  - Successful poll maps all fields correctly
  - HTTP 500 from device propagates as `Err`
  - `set_element(true)` issues the correct HTTP request body
  - Timeout returns `Err` (configure a very short timeout in the test)

## 3. `GenericHAEntityProvider` (geyser provider)

Maps arbitrary Home Assistant entity IDs to `GeyserState`. Talks to HA via the REST API (`/api/states/<entity_id>`). Covers any geyser already exposed in HA (GeyserWise, ESPHome, Shelly, etc.).

- [ ] 3.1 Define `GenericHAEntityConfig` struct:
  - `ha_base_url: String`, `ha_token: String`
  - `tank_temp_entity: String` (required)
  - `collector_temp_entity: Option<String>`
  - `pump_active_entity: Option<String>`
  - `heating_active_entity: Option<String>`
  - `element_kw: f32` (static — HA entities rarely expose live element power)
  - `tank_volume_l: f32` (static)
  - `set_element_service: Option<String>` (HA service call, e.g. `switch.turn_on`)
  - `set_pump_service: Option<String>`
  - `system: HeatingSystem`
- [ ] 3.2 Implement `get_state()`: GET `/api/states/<entity_id>` for each configured entity in parallel; map `state` field strings to the appropriate Rust types
- [ ] 3.3 Handle entity unavailability: if a required entity returns `"unavailable"` or `"unknown"`, return `Err`; optional entities return `None`
- [ ] 3.4 Implement `set_element(on: bool)`: POST to `/api/services/<domain>/<service>` with `entity_id` in the body; skip silently if `set_element_service` is `None`
- [ ] 3.5 Implement `set_pump(on: bool)`: same pattern for `set_pump_service`
- [ ] 3.6 Implement `capabilities()`: derive from which `Option` fields are `Some` in config (e.g. `CollectorTemp` only if `collector_temp_entity.is_some()`)
- [ ] 3.7 Implement `name()`: return `"Generic HA Entity"`
- [ ] 3.8 Implement `system()`: return the `system` field from config
- [ ] 3.9 Unit tests with `wiremock`:
  - All entities present and healthy → full `GeyserState` populated
  - `collector_temp_entity` is `None` → `capabilities()` omits `CollectorTemp`
  - HA returns `"unavailable"` for required tank temp entity → `Err`
  - `set_element(false)` with no `set_element_service` configured → succeeds silently (no HTTP call)

## 4. `GenericMqttProvider` (geyser provider)

Subscribes to MQTT topics for geyser telemetry; publishes command topics for control. Covers custom firmware (ESPHome, Tasmota, custom ESP32).

- [ ] 4.1 Define `MqttGeyserConfig` struct:
  - `broker: String`, `port: u16` (default: 1883)
  - `tank_temp_topic: String` (required)
  - `collector_temp_topic: Option<String>`
  - `pump_active_topic: Option<String>`
  - `heating_active_topic: Option<String>`
  - `element_cmd_topic: Option<String>` (publishes `"ON"` / `"OFF"`)
  - `pump_cmd_topic: Option<String>`
  - `element_kw: f32`, `tank_volume_l: f32`, `system: HeatingSystem` (static config)
  - `stale_timeout_secs: u32`
- [ ] 4.2 Connect to broker on construction; subscribe to all configured topics; store latest values in an `Arc<RwLock<MqttGeyserState>>` updated by the subscriber task
- [ ] 4.3 Implement `get_state()`: read from in-memory cache; return `Err` if no message received for `stale_timeout_secs`
- [ ] 4.4 Implement `set_element(on: bool)`: publish `"ON"` or `"OFF"` to `element_cmd_topic` (QoS 1); return `Err` if topic not configured
- [ ] 4.5 Implement `set_pump(on: bool)`: same pattern for `pump_cmd_topic`
- [ ] 4.6 Implement `capabilities()` and `name()` (return `"Generic MQTT Geyser"`) — derive capabilities from which topics are `Some`
- [ ] 4.7 Unit tests using an in-process MQTT broker stub:
  - Message on tank temp topic updates internal cache
  - `get_state()` after stale timeout returns `Err`
  - `set_element(true)` publishes the correct payload to the command topic
  - `set_element(true)` when `element_cmd_topic` is `None` returns `Err`

## 5. `VictronProvider` (PV provider)

Connects to Venus OS via local MQTT. Publishes a keep-alive every 30 seconds — required by Venus OS or it stops publishing (spec §6.1).

- [ ] 5.1 Define `VictronProviderConfig` per spec §6.1: `broker_host: String`, `broker_port: u16` (default: 1883), `portal_id: String`, `keepalive_interval_secs: u32` (default: 30)
- [ ] 5.2 Compute topic strings from `portal_id` at construction time:
  - `N/{portal_id}/system/0/Dc/Battery/Soc` → `battery_soc_pct`
  - `N/{portal_id}/system/0/Dc/Pv/Power` → `pv_power_w`
  - `N/{portal_id}/system/0/Ac/Grid/L1/Power` → `grid_power_w`
  - `N/{portal_id}/system/0/Dc/Battery/Power` → `battery_power_w`
  - `N/{portal_id}/system/0/Ac/Consumption/L1/Power` → `load_power_w`
- [ ] 5.3 Subscribe to all five topics on connection; store latest values in `Arc<RwLock<VictronState>>`
- [ ] 5.4 Spawn a background keep-alive task: every `keepalive_interval_secs`, publish `{}` to `R/{portal_id}/system/0/Serial` (QoS 0); cancel the task on `VictronProvider` drop
- [ ] 5.5 Implement `get_pv_state()`: read from cache; `battery_soc_pct` is required — return `Err` if not yet received or stale; all other fields are `Option`
- [ ] 5.6 Implement `capabilities()` per spec §6.1: `HashSet::from([PvPower, GridPower, BatteryPower, LoadPower])` — `BatteryCapacity` is NOT reported (not available via these topics)
- [ ] 5.7 Implement `name()`: return `"Victron Venus OS"`
- [ ] 5.8 Unit tests:
  - Messages on all five topics → full `PVSystemState` populated
  - Keep-alive task publishes to the correct topic at the configured interval
  - `get_pv_state()` when only SOC received → `Ok` with all power fields as `None`
  - `get_pv_state()` with no messages (stale) → `Err`
  - `capabilities()` does not include `BatteryCapacity`

## 6. `SunsynkProvider` (PV provider)

Pulls PV state from the Sunsynk HA integration entities via the HA REST API. All fields except SOC are optional.

- [ ] 6.1 Define `SunsynkProviderConfig` per spec §6.2: `ha_base_url: String`, `ha_token: String`, `soc_entity: String`, `pv_power_entity: Option<String>`, `grid_power_entity: Option<String>`, `battery_power_entity: Option<String>`, `load_power_entity: Option<String>`
- [ ] 6.2 Implement `get_pv_state()`: GET `/api/states/<entity_id>` for each configured entity (parallel fetches); parse numeric state strings to `f32`; return `Err` if `soc_entity` is unavailable
- [ ] 6.3 Handle HA `"unavailable"` / `"unknown"` states for optional entities as `None` (not `Err`)
- [ ] 6.4 Implement `capabilities()`: derived dynamically from which entity IDs are `Some` in config — `BatteryCapacity` is not in the Sunsynk entity set
- [ ] 6.5 Implement `name()`: return `"Sunsynk"`
- [ ] 6.6 Unit tests with `wiremock`:
  - All entities configured and returning values → all fields populated
  - `grid_power_entity` is `None` → `capabilities()` omits `GridPower`; `grid_power_w` is `None` in state
  - HA returns `"unavailable"` for SOC entity → `Err`
  - HA returns `"unavailable"` for an optional entity → `None` in state, no error

## 7. `GenericHAPVProvider` (PV provider)

Maps any HA entity IDs to `PVSystemState`. Covers Goodwe, Fronius, SMA, Sungrow, Fox ESS, Growatt, EG4, custom ESPHome, Shelly EM, and anything else already in HA.

- [ ] 7.1 Define `GenericHAPVConfig` per spec §6.3:
  - `ha_base_url: String`, `ha_token: String`
  - `soc_entity: String` (required)
  - `pv_power_entity: Option<String>`
  - `grid_power_entity: Option<String>`
  - `battery_power_entity: Option<String>`
  - `load_power_entity: Option<String>`
  - `battery_capacity_entity: Option<String>` (kWh)
  - `grid_export_is_positive: bool` (default: `false` — negative = export; flip sign if inverter uses opposite convention)
- [ ] 7.2 Implement `get_pv_state()`: parallel fetch of all configured entities; apply sign flip to `grid_power_w` if `grid_export_is_positive == true` (multiply by −1 so export is always negative in `PVSystemState`)
- [ ] 7.3 Handle unavailable optional entities as `None`; unavailable SOC entity returns `Err`
- [ ] 7.4 Implement `capabilities()`: derived from which entity IDs are `Some`; include `BatteryCapacity` only if `battery_capacity_entity.is_some()`
- [ ] 7.5 Implement `name()`: return `"Generic HA PV"`
- [ ] 7.6 Unit tests with `wiremock`:
  - Full config → all capabilities reported; all fields populated
  - `grid_export_is_positive: true` with HA returning `+500` → `grid_power_w` is `−500.0`
  - `battery_capacity_entity` is `None` → `BatteryCapacity` absent from `capabilities()`
  - `battery_capacity_entity` is `Some` and returns `"10.0"` → `battery_capacity_kwh` is `Some(10.0)`
  - SOC entity unavailable → `Err`; optional entity unavailable → `None`, no error

## 8. `GenericMqttPVProvider` (PV provider)

Subscribes to MQTT topics for PV telemetry. Covers custom firmware, non-HA setups, and any inverter with MQTT output.

- [ ] 8.1 Define `MqttPVConfig` per spec §6.4: `broker: String`, `port: u16`, `soc_topic: String`, `pv_power_topic: Option<String>`, `grid_power_topic: Option<String>`, `battery_power_topic: Option<String>`, `load_power_topic: Option<String>`, `grid_export_is_positive: bool`
- [ ] 8.2 Connect to broker, subscribe to all configured topics on construction; store latest values in `Arc<RwLock<MqttPVState>>`
- [ ] 8.3 Implement `get_pv_state()`: read from cache; apply sign flip for grid power if `grid_export_is_positive == true`; return `Err` if SOC value is absent or stale (`stale_timeout_secs`)
- [ ] 8.4 Implement `capabilities()`: derived from which topics are `Some`; `BatteryCapacity` is never reported (no capacity topic in this provider)
- [ ] 8.5 Implement `name()`: return `"Generic MQTT PV"`
- [ ] 8.6 Unit tests using an in-process MQTT stub:
  - Messages on all configured topics populate the full state
  - `get_pv_state()` with stale SOC → `Err`
  - `grid_export_is_positive: true` with raw payload `"300"` → `grid_power_w` is `Some(−300.0)`
  - `pv_power_topic` absent from config → `PvPower` not in `capabilities()`

## 9. Cross-provider integration tests

End-to-end tests pairing a provider with the core engines against a stubbed transport. Lives in `crates/smart-geyser-providers/tests/`.

- [ ] 9.1 `tests/geyserwala_integration.rs`: `wiremock` server mimicking Geyserwala API; assert `get_state()` and `set_element()` round-trips correctly
- [ ] 9.2 `tests/ha_entity_integration.rs`: `wiremock` server; assert partial configs (missing optional entities) still satisfy `GeyserProvider` trait and `capabilities()` is correct
- [ ] 9.3 `tests/mqtt_geyser_integration.rs`: in-process MQTT broker; assert `GenericMqttProvider` updates state on incoming messages and issues the correct publish on `set_element()`
- [ ] 9.4 `tests/victron_integration.rs`: in-process MQTT broker; assert `VictronProvider` populates all five fields, keep-alive is published at the configured interval, and stale detection works
- [ ] 9.5 `tests/sunsynk_integration.rs`: `wiremock` server; verify all optional-entity permutations produce the correct `capabilities()` set
- [ ] 9.6 `tests/ha_pv_integration.rs`: `wiremock` server; verify sign-flip logic for `grid_export_is_positive` and `BatteryCapacity` capability gating
- [ ] 9.7 `tests/mqtt_pv_integration.rs`: in-process MQTT stub; verify stale detection and sign-flip for grid power
- [ ] 9.8 Smoke test pairing `VictronProvider` stub with `OpportunityEngine` (from Phase 3): feed a battery-full + exporting state → engine emits `Opportunity` intent; feed SOC drop below threshold → engine stops

## 10. Phase 4 wrap-up

- [ ] 10.1 Inside the dev container: `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace` all green (per CLAUDE.md Docker-only rule)
- [ ] 10.2 Confirm `smart-geyser-core` has no new I/O or provider-specific imports — the providers crate is the only place with network I/O
- [ ] 10.3 Confirm all provider config field names use explicit-unit convention (`_c`, `_w`, `_kwh`, `_pct`, `_min`, `_secs`) per CLAUDE.md conventions
- [ ] 10.4 Tag commit `phase-4-complete`
- [ ] 10.5 Update [CLAUDE.md](../CLAUDE.md) with any provider-specific architectural decisions (MQTT client library chosen, Geyserwala API version targeted)
- [ ] 10.6 Open Phase 5 task file when starting that phase
