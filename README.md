# smart-geyser

Hardware-agnostic, learning controller for domestic water heaters (electric, solar-pumped, heat pump). Pairs with a battery/PV system when available so the geyser can act as opportunistic thermal storage.

The authoritative design document is [`smart-geyser-spec-v5_1.md`](smart-geyser-spec-v5_1.md). Phase work is tracked in [`tasks/`](tasks/).

## Repository layout

```
smart-geyser/
|-- crates/
|   `-- smart-geyser-core/   # pure logic, traits, models. No I/O.
|-- docker/
|   `-- Dockerfile.dev       # pinned Rust toolchain image
|-- docker-compose.yml       # `dev` service used for every build/test
|-- smart-geyser-spec-v5_1.md
`-- tasks/
```

## Docker-only development

All builds, tests, lints, and tooling run **inside the dev container**. Do not invoke `cargo`, `rustc`, `python`, `pip`, or `pytest` on the host. See [CLAUDE.md](CLAUDE.md#docker-only-development-hard-rule) for the rationale.

First-time setup:

```bash
docker compose build dev
```

Common commands (run from the repo root):

```bash
docker compose run --rm dev cargo build --workspace
docker compose run --rm dev cargo test --workspace
docker compose run --rm dev cargo test -p smart-geyser-core
docker compose run --rm dev cargo fmt --check
docker compose run --rm dev cargo clippy --workspace -- -D warnings
```

For an interactive shell inside the dev container:

```bash
docker compose run --rm dev bash
```

The repo is bind-mounted at `/workspace`, so edits made on the host appear immediately inside the container. Cargo's registry, git cache, and `target/` are stored in named Docker volumes so rebuilds are fast and the host's filesystem stays clean.
