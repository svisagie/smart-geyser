# Phase 6 — HA Integration

**Spec:** [../smart-geyser-spec-v5_1.md](../smart-geyser-spec-v5_1.md) §13 Phase 6, §8 (HA Entities), §7 (Service API), §1 (thin-client principle)

**Goal:** Build the Home Assistant custom integration — a thin Python client that proxies all state and control to the Rust service via local REST. No logic lives in the Python layer; HA sees sensors, binary sensors, and controls backed by the service API.

**Exit criteria:** The custom component loads in HA without errors, all entities from §8 appear and update, config flow guides a user through provider selection, and the Lovelace dashboard renders the full status including PV/opportunity fields.

---

## 1. Python package scaffolding

- [ ] 1.1 Create `ha-integration/custom_components/smart_geyser/` directory structure with `__init__.py`, `manifest.json`, `const.py`, `config_flow.py`, `coordinator.py`, `sensor.py`, `binary_sensor.py`, `number.py`, `switch.py`, `services.yaml`
- [ ] 1.2 `manifest.json`: set `domain`, `name`, `version`, `iot_class: "local_polling"`, `requirements`, `config_flow: true`
- [ ] 1.3 `const.py`: define `DOMAIN`, `DEFAULT_PORT`, `SCAN_INTERVAL`, and all entity unique-ID constants
- [ ] 1.4 Add `requirements.txt` / `pyproject.toml` for the integration package with dev dependencies (`pytest`, `pytest-homeassistant-custom-component`)

## 2. REST API client module

- [ ] 2.1 Create `api_client.py` — a simple async HTTP wrapper (using `aiohttp`, already available in HA) around the Rust service endpoints
- [ ] 2.2 Implement `get_status()` → parses the full status JSON (spec §7.2) into a dataclass
- [ ] 2.3 Implement `get_pv_state()` → parses `/api/pv-state` response
- [ ] 2.4 Implement `get_opportunity_log()` → parses `/api/opportunity-log` response
- [ ] 2.5 Implement `post_boost(until_iso: str)` and `post_setpoint(temp_c: float)` for control actions
- [ ] 2.6 Raise `CannotConnect` / `InvalidResponse` custom exceptions; caller handles them gracefully
- [ ] 2.7 Unit tests for the client using `aioresponses` to mock HTTP responses (happy path + error cases)

## 3. DataUpdateCoordinator

- [ ] 3.1 Create `coordinator.py` with a `SmartGeyserCoordinator(DataUpdateCoordinator)` that polls `/api/status` on `SCAN_INTERVAL` (default 30 s)
- [ ] 3.2 Store the parsed status dict as `coordinator.data`; all entity platforms read from this single snapshot
- [ ] 3.3 On `UpdateFailed`, log a warning and let HA mark entities unavailable — no retry logic needed

## 4. Config flow

- [ ] 4.1 Implement `SmartGeyserConfigFlow` with a host + port step; validate by calling `get_status()` and catching `CannotConnect`
- [ ] 4.2 Store `host` and `port` in `config_entry.data`
- [ ] 4.3 Options flow: allow changing the scan interval and opportunity max temp via UI
- [ ] 4.4 Translations: `strings.json` + `translations/en.json` for all config flow labels

## 5. Sensor entities (spec §8.1)

- [ ] 5.1 `sensor.smart_geyser_tank_temp` — tank_temp_c, device class `temperature`, unit `°C`
- [ ] 5.2 `sensor.smart_geyser_collector_temp` — collector_temp_c (optional, only if present in status)
- [ ] 5.3 `sensor.smart_geyser_battery_soc` — battery_soc_pct, device class `battery`, unit `%`
- [ ] 5.4 `sensor.smart_geyser_pv_power` — pv_power_w, device class `power`, unit `W`
- [ ] 5.5 `sensor.smart_geyser_grid_power` — grid_power_w, device class `power`, unit `W` (negative = export)
- [ ] 5.6 `sensor.smart_geyser_pv_opportunity_kwh_today` — device class `energy`, unit `kWh`
- [ ] 5.7 `sensor.smart_geyser_pv_opportunity_fraction_30d` — unit `%`, icon `mdi:solar-power`
- [ ] 5.8 `sensor.smart_geyser_grid_fraction_30d` — unit `%`, icon `mdi:transmission-tower`
- [ ] 5.9 `sensor.smart_geyser_next_predicted_use` — device class `timestamp`
- [ ] 5.10 `sensor.smart_geyser_events_today` — state class `total_increasing`

## 6. Binary sensor entities (spec §8.2)

- [ ] 6.1 `binary_sensor.smart_geyser_heating_active` — device class `running`
- [ ] 6.2 `binary_sensor.smart_geyser_pump_active` — device class `running`
- [ ] 6.3 `binary_sensor.smart_geyser_smart_stop_active` — device class `running`
- [ ] 6.4 `binary_sensor.smart_geyser_preheat_active` — device class `running`
- [ ] 6.5 `binary_sensor.smart_geyser_opportunity_active` — device class `running`, icon `mdi:solar-power-variant`

## 7. Number and switch controls (spec §8.3)

- [ ] 7.1 `number.smart_geyser_opportunity_max_temp` — range 60–75°C, step 0.5, writes via service call to Rust API
- [ ] 7.2 `number.smart_geyser_soc_full_threshold` — range 80–100%, step 1, writes via service call
- [ ] 7.3 `switch.smart_geyser_opportunity_enabled` — reads `opportunity_active` state; writes enable/disable via API

## 8. Services

- [ ] 8.1 `smart_geyser.boost` — accepts `duration_minutes` (int), calls `POST /api/boost`
- [ ] 8.2 `smart_geyser.set_setpoint` — accepts `temp_c` (float), calls `POST /api/setpoint`
- [ ] 8.3 Register services in `services.yaml` with field schemas; add translations in `strings.json`

## 9. Lovelace dashboard

- [ ] 9.1 Create `ha-integration/dashboards/smart_geyser.yaml` — a standalone Lovelace dashboard card bundle
- [ ] 9.2 Card layout: tank temp gauge, heating/pump/opportunity status badges, PV power + battery SOC stats, energy breakdown (solar thermal / PV / grid %), next predicted use + preheat start time
- [ ] 9.3 Include a button card for manual boost
- [ ] 9.4 Dashboard works with the default HA Lovelace (no custom card dependencies required for core view)

## 10. Phase 6 wrap-up

- [ ] 10.1 Run integration tests inside the Docker dev container: `docker compose run --rm dev pytest ha-integration/tests/`
- [ ] 10.2 Validate the manifest with HACS pre-check tooling
- [ ] 10.3 Tag commit `phase-6-complete`
- [ ] 10.4 Update [CLAUDE.md](../CLAUDE.md) with any integration decisions or lessons
- [ ] 10.5 Open Phase 7 task file when starting that phase
