# Phase 7 — Polish & Release

**Spec:** [../smart-geyser-spec-v5_1.md](../smart-geyser-spec-v5_1.md) §13 Phase 7, §12 (Open Questions — #7 crates.io, #8 load-shedding), §10 (Usage Scenarios — informs hardware guides), §6.5 (PV Provider Summary — informs provider guide)

**Goal:** Production-ready release. Load-shedding awareness (Eskom EskomSePush API), HACS listing, hardware setup guides for all usage scenarios (spec §10), provider configuration guide, crates.io publishing of `smart-geyser-core`, and a versioned GitHub release.

**Exit criteria:** The integration is listed on HACS, all documentation covers the scenarios in spec §10, load-shedding suppression is implemented and tested, `smart-geyser-core` is published to crates.io, and a tagged GitHub release exists with a changelog and pre-built Docker image.

---

## 1. Load-shedding integration (South Africa — spec §12 open question #8)

Load-shedding is scheduled grid outages managed by Eskom (and municipal distributors). During a scheduled outage the inverter runs the home from battery — opportunity heating must not discharge the battery when there is no grid to recharge from.

- [ ] 1.1 Define `LoadSheddingConfig` struct: `enabled: bool`, `provider: LoadSheddingProvider` (enum: `EskomSePush { api_key: String, area_id: String }`, `Manual { schedule: Vec<LoadSheddingWindow> }`)
- [ ] 1.2 Define `LoadSheddingWindow` struct: `starts_at: DateTime<Utc>`, `ends_at: DateTime<Utc>`
- [ ] 1.3 Implement `LoadSheddingCalendar`: fetches the EskomSePush API schedule for the configured area and caches it (refresh every 4 hours; use cached value on fetch failure to avoid disruption on API downtime)
- [ ] 1.4 Integrate into `OpportunityEngine::tick`: if `loadshedding_calendar.is_outage_now_or_imminent(now, lookahead_min: 30)` → suppress opportunity heating (battery must be preserved as backup power)
- [ ] 1.5 Integrate into `DecisionEngine::tick`: if outage is imminent (within `lookahead_min`), pre-heat to setpoint NOW regardless of normal schedule (ensure hot water for the outage duration) — treat as an emergency preheat intent, which the priority order places above `Preheat` but below `Boost`
- [ ] 1.6 Add `load_shedding: Option<LoadSheddingConfig>` to `EngineConfig` (default: `None` — disabled)
- [ ] 1.7 Unit tests:
  - Outage active now → opportunity suppressed; normal preheat suppressed
  - Outage starts in 25 min (lookahead 30 min) → emergency preheat fires; opportunity suppressed
  - Outage starts in 45 min (lookahead 30 min) → normal operation (too far away)
  - EskomSePush fetch fails → cached schedule used; no panic
  - `Manual` schedule provider: explicitly specified windows are respected
- [ ] 1.8 Add `load_shedding_imminent` boolean field to the `/api/status` response and to the corresponding HA binary sensor
- [ ] 1.9 Add `binary_sensor.smart_geyser_load_shedding_imminent` to the HA integration (Phase 6 entity list addendum)

## 2. HACS listing

- [ ] 2.1 Create `hacs.json` in the repo root: `{"name": "Smart Geyser Controller", "category": "integration"}`
- [ ] 2.2 Ensure the repository meets HACS requirements: public repo, at least one release tag, `custom_components/smart_geyser/manifest.json` present, `hacs.json` present
- [ ] 2.3 Submit the repository to the HACS default store (open a PR against `hacs/default`) — link PR in task notes once opened
- [ ] 2.4 Verify the integration appears correctly in the HACS UI (category, description, version, downloads) after listing
- [ ] 2.5 Add a HACS badge to the repo `README.md`

## 3. Hardware setup guides

One guide per usage scenario from spec §10. Lives in `docs/hardware/`.

- [ ] 3.1 `docs/hardware/electric-only-no-pv.md` — scenario §10.1: electric geyser, no PV, pure pattern scheduling. Covers: hardware requirements, Geyserwala/GeyserWise/ESPHome wiring, config snippet, expected HA entities, troubleshooting tips
- [ ] 3.2 `docs/hardware/solar-pumped-no-pv-battery.md` — scenario §10.2: solar thermal pump controlled by Geyserwala, no PV battery. Covers: pump wiring (12V DC vs 220V AC), collector sensor placement, `HeatingSystem::SolarPumped` config, smart-stop behaviour
- [ ] 3.3 `docs/hardware/solar-pumped-victron.md` — scenario §10.3: solar thermal + Victron battery/PV. Covers: Venus OS MQTT setup, `portal_id` retrieval, keep-alive requirement, opportunity heating behaviour, expected dashboard view
- [ ] 3.4 `docs/hardware/electric-sunsynk.md` — scenario §10.4: electric-only geyser + Sunsynk/Deye PV + battery. Covers: HA Sunsynk integration setup, entity ID mapping, sign convention verification, tuning `soc_full_threshold_pct`
- [ ] 3.5 `docs/hardware/heatpump-victron.md` — scenario §10.5: heat pump + Victron. Covers: COP configuration, `effective_cop` impact on lead time calculation, expected opportunity kWh multiplier (~3.5× vs resistive element), tempering valve reminder
- [ ] 3.6 Each guide includes: prerequisites, step-by-step config, a sample `config.yaml` snippet, and a "verify it's working" checklist

## 4. Provider configuration guide

- [ ] 4.1 Create `docs/providers.md` covering all providers from spec §6.5: Victron, Sunsynk, GenericHAPV (with inverter-specific entity ID lookup tables for Goodwe, Fronius, SMA, Sungrow, Growatt, Fox ESS, Shelly EM), GenericMQTTPV
- [ ] 4.2 Include a `capabilities()` comparison table showing which PV data each provider exposes and which opportunity paths (A/B/C) it enables
- [ ] 4.3 Include a sign convention section explaining `grid_export_is_positive` and how to verify the correct value for a given inverter
- [ ] 4.4 Include geyser provider docs: Geyserwala Connect API version compatibility, GenericHAEntity entity ID discovery, GenericMQTT topic format expectations

## 5. `smart-geyser-core` crates.io publishing (spec §12 open question #7)

- [ ] 5.1 Review `crates/smart-geyser-core/Cargo.toml` for publish readiness: `description`, `license`, `repository`, `keywords`, `categories`, `readme` fields all set
- [ ] 5.2 Ensure `smart-geyser-core` has no path dependencies that would block publishing (it must depend only on crates.io crates)
- [ ] 5.3 Verify `cargo doc --no-deps -p smart-geyser-core` produces clean output with no warnings; check all public items are documented
- [ ] 5.4 Add a `crates/smart-geyser-core/README.md` with: project description, quick example (trait usage), link to full docs and repo
- [ ] 5.5 Dry-run publish: `docker compose run --rm dev cargo publish -p smart-geyser-core --dry-run`
- [ ] 5.6 Publish: `docker compose run --rm dev cargo publish -p smart-geyser-core` (requires crates.io token in environment)
- [ ] 5.7 Verify the crate appears on crates.io and `cargo add smart-geyser-core` works in a fresh project

## 6. End-to-end validation

- [ ] 6.1 Deploy the add-on to a real (or Home Assistant OS in a VM) HA instance and run through each scenario from spec §10 as a manual acceptance checklist
- [ ] 6.2 Confirm all HA entities appear, update, and are correctly classified (units, device class, state class)
- [ ] 6.3 Trigger a manual boost via HA service call → verify element turns on → boost expires → element turns off
- [ ] 6.4 If a real Victron or Sunsynk system is available: verify opportunity heating fires at correct SOC threshold and stops when SOC drops
- [ ] 6.5 Verify legionella safety override fires after the configured interval on a simulated cold tank (can be done via API manipulation)
- [ ] 6.6 Verify load-shedding suppression using the `Manual` schedule provider (set a window 10 minutes in the future, observe opportunity suppression and emergency preheat)

## 7. GitHub release & changelog

- [ ] 7.1 Write `CHANGELOG.md` with sections for each phase's key additions (Phase 1–7), formatted for HACS display
- [ ] 7.2 Bump version to `1.0.0` in: `crates/smart-geyser-core/Cargo.toml`, `crates/smart-geyser-providers/Cargo.toml`, `crates/smart-geyser-service/Cargo.toml`, `ha-integration/custom_components/smart_geyser/manifest.json`, `addon/config.yaml`
- [ ] 7.3 Tag `v1.0.0` and push to GitHub; let CI build the production Docker image and attach it as a release asset
- [ ] 7.4 Create a GitHub Release from the tag: paste the Phase 7 section of `CHANGELOG.md` as release notes; attach the Docker image tarball
- [ ] 7.5 Confirm HACS picks up the new version (tag visible in HACS update check)

## 8. Phase 7 wrap-up

- [ ] 8.1 All open questions from spec §12 that are addressed in this phase are marked resolved in the spec
- [ ] 8.2 `cargo test --workspace` still green after all changes
- [ ] 8.3 Update [CLAUDE.md](../CLAUDE.md) to reflect that the project is now in maintenance / post-release state
- [ ] 8.4 Tag commit `phase-7-complete`
