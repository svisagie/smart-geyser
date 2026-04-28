# Phase 1 — Core Library

**Spec:** [../smart-geyser-spec-v5_1.md](../smart-geyser-spec-v5_1.md) §13 Phase 1, §2 (PV model), §9 (EngineConfig)

**Goal:** Lay down the hardware-agnostic foundation. No I/O, no async runtime concerns yet — just the trait surface, data models, and the pure heat-calculation math, all unit-tested.

**Exit criteria:** `cargo test -p smart-geyser-core` passes. The crate compiles standalone, exports the public traits and models, and has zero runtime dependencies on a specific provider.

---

## 0. Docker dev environment (must come first — see CLAUDE.md "Docker-only development")

All cargo/rustc/etc. commands run **inside the dev container**, never on the host. This section makes that possible.

- [x] 0.1 Create `docker/Dockerfile.dev` based on a pinned `rust:<version>-slim` (or `rust:<version>-bookworm`) image
- [x] 0.2 Install build essentials in the image: `pkg-config`, `libssl-dev`, `ca-certificates`, `git`, plus `rustfmt` + `clippy` components
- [x] 0.3 Add cargo registry/git/target cache volumes so rebuilds are fast (`/usr/local/cargo/registry`, `/usr/local/cargo/git`, and a workspace-scoped `target/`)
- [x] 0.4 Create `docker-compose.yml` at repo root with a `dev` service: builds `Dockerfile.dev`, bind-mounts the repo to `/workspace`, sets `WORKDIR /workspace`, declares the cache volumes from 0.3
- [x] 0.5 Verify the workflow with a smoke test: `docker compose build dev && docker compose run --rm dev rustc --version` succeeds
- [x] 0.6 Add `.dockerignore` (target/, .git/, IDE files) so build context stays small
- [x] 0.7 Document the workflow in repo `README.md` — pointing devs at CLAUDE.md's Docker section

## 1. Workspace + crate scaffolding (run inside the dev container)

- [x] 1.1 Create root `Cargo.toml` workspace with `crates/*` members
- [x] 1.2 Create `crates/smart-geyser-core/` crate with `Cargo.toml` and `src/lib.rs`
- [x] 1.3 Add baseline dependencies: `chrono` (with `serde`), `serde`, `serde_json`, `anyhow`, `thiserror`, `async-trait`
- [x] 1.4 Add dev-dependencies: `tokio` (for async tests), `pretty_assertions`, `rstest`
- [x] 1.5 Set up `rustfmt.toml` and `clippy.toml` at workspace root (pedantic lints on)
- [x] 1.6 Add `.gitignore` (target/, IDE files)
- [ ] 1.7 `git init` and first commit ("Phase 1 scaffolding")
- [ ] 1.8 Confirm `docker compose run --rm dev cargo build --workspace` succeeds (empty workspace builds cleanly)

## 2. Domain models (`models.rs`)

- [x] 2.1 Define `GeyserState` struct (tank_temp_c, collector_temp_c, pump_active, heating_active, element_kw, tank_volume_l, timestamp)
- [x] 2.2 Define `PVSystemState` exactly per spec §2.1 (required `battery_soc_pct`, optional everything else)
- [x] 2.3 Define `PVCapability` enum per spec §2.2
- [x] 2.4 Define `OpportunityConfig` per spec §3.5 (with documented defaults)
- [x] 2.5 Define `SolarWindow` struct per spec §3.4 (impl block can be a stub returning `unimplemented!()` — Phase 3 fills it in)
- [x] 2.6 Define `EngineConfig` per spec §9 with `Default` impl matching the documented defaults
- [x] 2.7 Add `Serialize`/`Deserialize` derives where the spec implies on-the-wire use
- [x] 2.8 Unit tests: round-trip serde for every public model; default values match the spec

## 3. `HeatingSystem` enum (`system.rs`)

- [x] 3.1 Define `HeatingSystem` enum: `ElectricOnly`, `SolarPumped { pump_voltage: PumpVoltage }`, `HeatPump { cop_nominal: f32, live_cop: Option<f32> }`
- [x] 3.2 Define `PumpVoltage` enum (`Dc12V`, `Ac220V`)
- [x] 3.3 Helper methods: `effective_cop(&self) -> f32`, `is_solar_pumped(&self) -> bool`, etc.
- [x] 3.4 Unit tests covering each variant + `effective_cop` (live_cop overrides nominal; ElectricOnly returns 1.0)

## 4. Geyser provider trait (`provider.rs`)

- [x] 4.1 Define `GeyserCapability` enum (TankTemp, CollectorTemp, PumpControl, ElementControl, BoostControl, FaultStatus)
- [x] 4.2 Define `GeyserProvider` trait (`async_trait`): `get_state`, `set_element`, `set_pump`, `capabilities`, `name`, `system(&self) -> HeatingSystem`
- [x] 4.3 Add `MockGeyserProvider` in `#[cfg(test)]` for downstream Phase 2/3 tests
- [x] 4.4 Unit test: mock provider satisfies the trait and returns sensible values

## 5. PV provider trait (`pv_provider.rs`)

- [x] 5.1 Define `PVSystemProvider` trait per spec §2.3 (`get_pv_state`, `capabilities`, `name`)
- [x] 5.2 Add `MockPVProvider` in `#[cfg(test)]` configurable per-capability
- [x] 5.3 Unit test: mock provider with only SOC capability still satisfies the contract; richer mocks expose more capabilities

## 6. Heat calculation math (`heat_calc.rs`)

- [x] 6.1 Implement `heat_lead_time_minutes(state: &GeyserState, target_temp_c: f32, system: &HeatingSystem) -> u32`
  - Pure function. Uses `Q = m × c × ΔT` with `c = 4186 J/kg·K`, water density 1 kg/L
  - Divides by `element_kw × effective_cop` to convert thermal energy to wall-clock time
- [x] 6.2 Implement `energy_to_heat_kwh(volume_l: f32, delta_t_c: f32) -> f32` helper
- [x] 6.3 Implement `thermal_energy_stored_kwh(volume_l: f32, temp_c: f32, baseline_c: f32) -> f32`
- [x] 6.4 Unit tests with known-good values:
  - 150 L tank, 20°C → 60°C, 3 kW element, COP 1.0 ⇒ ~140 min
  - 150 L tank, 20°C → 60°C, 3 kW element, COP 3.5 ⇒ ~40 min (heat pump case)
  - 150 L tank, 20°C → 70°C ⇒ ~9 kWh stored (sanity-check the spec §11 claim)
  - Edge cases: target ≤ current temp returns 0; zero element_kw returns u32::MAX

## 7. Crate-level public API (`lib.rs`)

- [x] 7.1 Re-export the public surface: traits, models, enums, heat_calc functions
- [x] 7.2 Module-level docs explaining the trait-based architecture (link to spec §1)
- [ ] 7.3 `cargo doc --no-deps` produces clean output with no warnings

## 8. Phase 1 wrap-up

- [x] 8.1 Inside the dev container: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` all green
- [x] 8.2 README stub at repo root describing the Docker-based build/test workflow (link to CLAUDE.md)
- [ ] 8.3 Tag commit `phase-1-complete`
- [ ] 8.4 Update [CLAUDE.md](../CLAUDE.md) with any architectural decisions made during the phase
