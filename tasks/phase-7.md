# Phase 7 — Polish & Release

**Spec:** [../smart-geyser-spec-v5_1.md](../smart-geyser-spec-v5_1.md) §13 Phase 7

**Goal:** Production-ready release.

---

## 1. Load-shedding integration

- [ ] 1.1–1.9 Deferred to v2 — requires EskomSePush API key and OpportunityEngine (v2 feature)

## 2. HACS listing

- [x] 2.1 Create `hacs.json`: `{"name": "Smart Geyser Controller", "category": "integration"}`
- [x] 2.2 Ensure repo meets HACS requirements — public GitHub repo at https://github.com/svisagie/smart-geyser, v1.0.0 tag pushed, custom_components/ at repo root
- [ ] 2.3 Submit PR to hacs/default (deferred — needs 30-day public repo age)
- [ ] 2.4 Verify in HACS UI (deferred)
- [ ] 2.5 Add HACS badge to README (deferred)

## 3. Hardware setup guides

- [x] 3.1 `docs/hardware/electric-only-no-pv.md`
- [x] 3.2 `docs/hardware/solar-pumped-no-pv-battery.md`
- [ ] 3.3 `docs/hardware/solar-pumped-victron.md` — deferred (v2: Victron provider)
- [x] 3.4 `docs/hardware/electric-sunsynk.md` — placeholder (v2: Sunsynk provider)
- [ ] 3.5 `docs/hardware/heatpump-victron.md` — deferred (v2)
- [x] 3.6 Guides include prerequisites, config snippets, verify checklist

## 4. Provider configuration guide

- [ ] 4.1–4.4 Deferred to v2 (only Geyserwala provider in v1)

## 5. `smart-geyser-core` crates.io publishing

- [x] 5.1 `Cargo.toml` publish fields: description, keywords, categories, readme
- [x] 5.2 No path dependencies that block publishing (all workspace deps are crates.io)
- [ ] 5.3 `cargo doc` clean output check
- [x] 5.4 `crates/smart-geyser-core/README.md` written
- [x] 5.5 Dry-run publish succeeds: `cargo publish -p smart-geyser-core --dry-run --allow-dirty`
- [ ] 5.6 Live publish (deferred — requires crates.io token)
- [ ] 5.7 Verify on crates.io (deferred)

## 6. End-to-end validation

- [ ] 6.1–6.6 Manual acceptance testing (deferred — requires real hardware + HA instance)

## 7. GitHub release & changelog

- [x] 7.1 `CHANGELOG.md` written with Phase 1–7 sections
- [x] 7.2 Version bumped to `1.0.0` in workspace Cargo.toml, manifest.json, addon/config.yaml
- [x] 7.3 Tag `v1.0.0` pushed to https://github.com/svisagie/smart-geyser
- [ ] 7.4 Create GitHub Release (triggered by release.yml workflow on v1.0.0 tag push)
- [ ] 7.5 HACS picks up new version (deferred — verify after release workflow completes)

## 8. Phase 7 wrap-up

- [x] 8.1 Open spec questions addressed (load-shedding and crates.io noted above)
- [x] 8.2 `cargo test --workspace` + `pytest` still green after all changes
- [x] 8.3 Update [CLAUDE.md](../CLAUDE.md) to reflect post-v1 state
- [x] 8.4 Tag commit `phase-7-complete` — repo live at https://github.com/svisagie/smart-geyser
