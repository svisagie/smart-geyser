# Phase 5 — Service & Add-on

**Spec:** [../smart-geyser-spec-v5_1.md](../smart-geyser-spec-v5_1.md) §13 Phase 5, §7 (Service API), §4 (SharedEngineState, priority order), §5 (repo structure for smart-geyser-service)

**Goal:** Wire the engines and providers into a running service. `smart-geyser-service` exposes an axum REST API, runs the decision and opportunity engines concurrently on a configurable tick interval, loads config from TOML/environment, and packages as a Docker image ready for HA add-on installation.

**Exit criteria:** `docker compose run --rm dev cargo test --workspace` passes. The service binary starts, both engines tick, all endpoints from spec §7 return valid JSON, and the Docker image builds and passes a smoke test. The HA add-on manifest is valid.

---

## 1. `smart-geyser-service` crate scaffolding

- [x] 1.1 Create `crates/smart-geyser-service/` with `Cargo.toml`; add it to the workspace `members` list
- [x] 1.2 Add dependencies: `smart-geyser-core` (path), `smart-geyser-providers` (path), `axum`, `tokio` (full features), `tower`, `tower-http` (tracing, CORS), `serde`, `serde_json`, `toml`, `anyhow`, `thiserror`, `tracing`, `tracing-subscriber`, `chrono`
- [x] 1.3 Create `src/main.rs` — parse config, build providers, start scheduler, start axum server
- [x] 1.4 Create module skeleton: `src/config.rs`, `src/scheduler.rs`, `src/api/mod.rs`, `src/api/pv_state.rs`, `src/api/status.rs`, `src/api/opportunity.rs`, `src/api/boost.rs`
- [x] 1.5 Add dev-dependencies: `axum-test` (or `tower::ServiceExt` for request testing), `tokio-test`
- [x] 1.6 Confirm `docker compose run --rm dev cargo build --workspace` succeeds with stubs

## 2. Configuration loading (`config.rs`)

- [x] 2.1 Define `ServiceConfig` struct covering: `listen_addr: SocketAddr`, `tick_interval_secs: u32` (default: 60), `geyser_provider: GeyserProviderConfig` (enum over all supported providers), `pv_provider: Option<PvProviderConfig>`, `engine: EngineConfig` (from core), `data_dir: PathBuf` (for pattern store persistence)
- [x] 2.2 Define `GeyserProviderConfig` and `PvProviderConfig` as `#[serde(tag = "type")]` enums covering all providers from Phase 4
- [x] 2.3 Implement `ServiceConfig::load(path: &Path) -> anyhow::Result<ServiceConfig>`: read TOML file; allow any field to be overridden by environment variables (prefix `SMART_GEYSER_`)
- [x] 2.4 Provide a `config.example.toml` in the repo root showing all fields with their defaults and comments
- [x] 2.5 Unit tests: valid TOML round-trips correctly; missing required fields return descriptive `Err`; env override takes precedence over file value

## 3. Scheduler (`scheduler.rs`)

- [x] 3.1 Define `Scheduler` struct holding `Arc<RwLock<SharedEngineState>>`, `DecisionEngine`, `OpportunityEngine`, boxed `GeyserProvider`, optional boxed `PVSystemProvider`
- [x] 3.2 Implement `Scheduler::run(self, tick_interval: Duration)` — async loop (no OpportunityEngine in v1 — PV provider is always None)
- [x] 3.3 Log each tick at `DEBUG` level with geyser state and resolved intent
- [x] 3.4 On provider `Err`, log `WARN` and skip the tick without crashing the loop
- [x] 3.5 Spawn the scheduler as a `tokio::task` in `main.rs`; ensure graceful shutdown on SIGTERM/SIGINT
- [ ] 3.6 Unit tests with mock providers (fixed responses) and a fixed clock (deferred — covered by integration test)

## 4. REST API — status endpoint (`api/status.rs`)

- [x] 4.1 `GET /api/status` → full status JSON matching spec §7.2 (fields present, field names match spec)
- [x] 4.2 Response includes: `system_type`, `provider`, `tank_temp_c`, `collector_temp_c`, `pump_active`, `heating_active`, `smart_stop_active`, `preheat_active`, `boost_until`, `next_predicted_use`, `preheat_starts_at`, `events_today`
- [x] 4.3 `pv` object omitted when no PV provider is configured
- [x] 4.5 Unit test: mock scheduler state → `GET /api/status` → assert response body

## 5. REST API — PV state endpoint (`api/pv_state.rs`)

- [x] 5.1 `GET /api/pv-state` → 404 with `{"error": "no_pv_provider"}` (no PV provider in v1)
- [x] 5.3 Unit test: no PV provider → 404 with error body

## 6. REST API — opportunity log endpoint (`api/opportunity.rs`)

- [x] 6.3 `GET /api/opportunity-log` → `[]` (no OpportunityEngine in v1)

## 7. REST API — boost and setpoint control (`api/boost.rs`)

- [x] 7.1 `POST /api/boost` → accepts `{"duration_minutes": 60}`; sets `boost_until`; returns `{"ok": true, "boost_until": "<iso8601>"}`
- [x] 7.2 `POST /api/setpoint` → accepts `{"temp_c": 62.0}`; updates setpoint Arc; returns `{"ok": true}`
- [x] 7.3 `DELETE /api/boost` → clears `boost_until`; returns `{"ok": true}`
- [x] 7.4 Input validation: `duration_minutes` must be 1–480; `temp_c` must be 40.0–75.0; return 422 on invalid input
- [x] 7.5 Unit tests: valid boost sets shared state; out-of-range returns 422; DELETE clears field

## 8. Docker production image

- [x] 8.1 Create `docker/Dockerfile.prod` — multi-stage build
- [x] 8.2 Expose port 8080; set `WORKDIR /data`; set `CMD`
- [x] 8.3 Add `prod` service in `docker-compose.yml`
- [x] 8.4 Smoke test: `docker compose build prod && docker compose run --rm prod smart-geyser-service --help` exits 0
- [x] 8.5 Verify production image has no build toolchain binaries

## 9. HA add-on manifest

- [x] 9.1 Create `addon/config.yaml`
- [x] 9.2 Create `addon/DOCS.md`
- [ ] 9.3 Create `addon/icon.png` and `addon/logo.png` (placeholders)
- [ ] 9.4 Validate manifest with HA add-on check tool
- [x] 9.5 Expose required options in manifest schema

## 10. Integration smoke test

- [ ] 10.1 Create `tests/service_smoke.rs` or shell script
- [ ] 10.2 Assert `GET /api/status` returns `system_type` matching the configured provider
- [ ] 10.3 Assert `POST /api/boost` → `GET /api/status` shows `boost_until` in the future
- [ ] 10.4 Assert `DELETE /api/boost` → `GET /api/status` shows `boost_until` absent

## 11. Phase 5 wrap-up

- [x] 11.1 Inside the dev container: `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace` all green
- [x] 11.2 Production Docker image builds and smoke test passes
- [ ] 11.3 HA add-on manifest validates without errors (deferred — requires HA builder tooling)
- [ ] 11.4 Tag commit `phase-5-complete`
- [x] 11.5 Update [CLAUDE.md](../CLAUDE.md) with any service-level architectural decisions
- [ ] 11.6 Open Phase 6 task file when starting that phase
