# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Repository state

**Phase 1 and Phase 2 are complete.** `smart-geyser-core` is fully implemented: domain models, trait surfaces, heat math, event detection, pattern store, shared state, decision engine, and a 14-day integration smoke test. 67 tests pass; fmt, clippy (-D warnings), and doc-tests are all clean. Phase 3 (opportunity engine + PV integration) is next.

## Source-of-truth documents

- **Spec:** [smart-geyser-spec-v5_1.md](smart-geyser-spec-v5_1.md) — authoritative. Every implementation decision should be traceable back to a section here. Don't drift from it without updating it.
- **Task tracker:** [tasks/](tasks/) — phase-by-phase checklist. **Read the relevant phase file before starting work**, and **mark items complete (`[x]`) immediately** when finished — don't batch updates. New phases get their own file when their predecessor completes.

## High-level architecture (target — most of this is not built yet)

The system is a Rust workspace with three crates plus a Python HA integration:

- `smart-geyser-core` — pure logic, no I/O. Two trait surfaces: `GeyserProvider` (the heating hardware) and `PVSystemProvider` (the inverter/battery). The two are **independent** — you can have one, the other, both, or neither.
- `smart-geyser-providers` — concrete implementations of the traits (Geyserwala, Victron, Sunsynk, generic HA-entity, generic MQTT).
- `smart-geyser-service` — axum REST API + scheduler that runs the engines.
- HA Python integration — thin client over the local REST API.

Two engines run concurrently inside the service and coordinate through `SharedEngineState` (Arc/RwLock):

1. **`decision_engine`** — pattern-based pre-heat scheduling and smart-stop. Learns from a time-of-day histogram of detected hot-water-use events.
2. **`opportunity_engine`** — fires the element when the battery is full and PV is exporting surplus, treating the geyser as thermal storage.

The single most important design rule (spec §3.6): **smart-stop yields to opportunity heating**. Smart-stop exists to prevent grid-funded waste, not to block free PV energy. Element-control priority is `BOOST > FAULT > OPPORTUNITY > PREHEAT > IDLE`.

## Docker-only development (hard rule)

**All builds, tests, lints, and tooling must run inside Docker containers.** Do not invoke `cargo`, `rustc`, `python`, `pip`, `pytest`, etc. directly on the host. The host is Windows; the target runs in Linux containers (HA add-on); building on bare metal will silently diverge from CI and from production.

This applies to every phase. If a tool you need isn't in the dev image yet, add it to the `Dockerfile.dev` rather than installing it on the host. The first Phase 1 task is to stand up that image and the `docker compose` workflow — until then, no code runs.

Workflow once the dev image exists (commands run from the repo root):

```
docker compose build dev                              # build/refresh the dev image
docker compose run --rm dev cargo build --workspace
docker compose run --rm dev cargo test --workspace
docker compose run --rm dev cargo test -p smart-geyser-core              # one crate
docker compose run --rm dev cargo test -p smart-geyser-core heat_calc    # one module's tests
docker compose run --rm dev cargo fmt --check
docker compose run --rm dev cargo clippy --workspace -- -D warnings
```

For an interactive shell inside the dev container: `docker compose run --rm dev bash`. The repo is bind-mounted, so edits on the host show up immediately inside the container — but execution stays in the container.

These commands won't work until Phase 1 §1 (Docker dev environment + workspace scaffolding) is done. Don't claim "tests pass" before then, and don't bypass the container by running `cargo` on the host to "just check quickly."

## Conventions

- The `smart-geyser-core` crate must stay free of provider-specific code and free of I/O. Anything async-runtime-coupled lives in the service crate. Keep traits in core, implementations out.
- All units in models are explicit (`_c`, `_w`, `_kwh`, `_pct`, `_min`). Maintain that — don't introduce ambiguous numeric fields.
- `PVSystemState` has exactly **one required field** (`battery_soc_pct`); everything else is `Option`. Provider implementations populate what they can and report it via `capabilities()`. The opportunity engine degrades gracefully through Path A → B → C based on what's available (spec §3.2).
- Defaults in `OpportunityConfig` and `EngineConfig` are spec-defined — match them exactly in `Default` impls, don't invent new ones.

## Architectural decisions (Phase 1–2)

- `PatternStore` uses `Vec<f32>` (not `[f32; 168]`) for the histogram buckets — serde 1.x does not implement `Serialize`/`Deserialize` for fixed arrays larger than ~32 elements.
- `tokio` is a main dependency of `smart-geyser-core` (not just dev), because `shared_state.rs` uses `tokio::sync::RwLock` in non-test code.
- The decision engine test `make_engine` computes `lead_time` dynamically with `heat_lead_time_minutes` rather than a hardcoded constant — the preheat look-ahead depends on tank temp, which varies per test.
- Integration test ticks at 06:30 (not 06:00) so that `look_ahead = 06:30 + ~38 min = 07:08` falls in the hour-7 bucket where showers were recorded, giving probability ≥ threshold.
- `is_some_and` (Rust 1.70+) is preferred over `map().unwrap_or(false)` per clippy pedantic.

## Working style for this repo

- Build phase by phase. Don't start Phase N+1 until Phase N's exit criteria are green.
- For UI/service work, the spec's API shape (§7) is the contract — don't change response field names without updating the spec.
- When something in the spec is genuinely unclear or contradicts itself, ask before deciding. Don't silently choose.
