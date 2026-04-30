# Phase 6 — HA Integration

**Spec:** [../smart-geyser-spec-v5_1.md](../smart-geyser-spec-v5_1.md) §13 Phase 6, §8 (HA Entities), §7 (Service API), §1 (thin-client principle)

**Goal:** Build the Home Assistant custom integration — a thin Python client that proxies all state and control to the Rust service via local REST. No logic lives in the Python layer; HA sees sensors, binary sensors, and controls backed by the service API.

**Exit criteria:** The custom component loads in HA without errors, all entities from §8 appear and update, config flow guides a user through provider selection, and the Lovelace dashboard renders the full status including PV/opportunity fields.

---

## 1. Python package scaffolding

- [x] 1.1 Create `ha-integration/custom_components/smart_geyser/` directory structure with `__init__.py`, `manifest.json`, `const.py`, `config_flow.py`, `coordinator.py`, `sensor.py`, `binary_sensor.py`, `number.py`, `switch.py`, `services.yaml`
- [x] 1.2 `manifest.json`: set `domain`, `name`, `version`, `iot_class: "local_polling"`, `requirements`, `config_flow: true`
- [x] 1.3 `const.py`: define `DOMAIN`, `DEFAULT_PORT`, `SCAN_INTERVAL`, and all entity unique-ID constants
- [x] 1.4 Add `requirements-dev.txt` for the integration package with dev dependencies (`pytest`, `pytest-homeassistant-custom-component`)

## 2. REST API client module

- [x] 2.1 Create `api_client.py` — async HTTP wrapper (using `aiohttp`) around the Rust service endpoints
- [x] 2.2 Implement `get_status()` → parses the full status JSON into a dataclass
- [x] 2.3 `get_pv_state()` not needed — coordinator reads pv field from status
- [x] 2.4 `get_opportunity_log()` not needed in v1 (returns [])
- [x] 2.5 Implement `post_boost()`, `delete_boost()`, `post_setpoint()` for control actions
- [x] 2.6 Raise `CannotConnect` / `InvalidResponse` custom exceptions
- [x] 2.7 Unit tests for the client using `aioresponses` (happy path + error cases)

## 3. DataUpdateCoordinator

- [x] 3.1 Create `coordinator.py` with `SmartGeyserCoordinator(DataUpdateCoordinator)` polling `/api/status`
- [x] 3.2 Store parsed status as `coordinator.data`; all entity platforms read from this single snapshot
- [x] 3.3 On `UpdateFailed`, log warning and let HA mark entities unavailable

## 4. Config flow

- [x] 4.1 Implement `SmartGeyserConfigFlow` with host + port step; validate by calling `get_status()`
- [x] 4.2 Store `host` and `port` in `config_entry.data`
- [ ] 4.3 Options flow: allow changing scan interval (deferred — not critical for v1)
- [x] 4.4 Translations: `strings.json` + `translations/en.json` for all config flow labels

## 5. Sensor entities (spec §8.1)

- [x] 5.1 `sensor.smart_geyser_tank_temp` — tank_temp_c, device class `temperature`, unit `°C`
- [x] 5.2 `sensor.smart_geyser_collector_temp` — collector_temp_c (optional, None if absent)
- [x] 5.3 `sensor.smart_geyser_battery_soc` — battery_soc_pct (from pv object, None in v1)
- [x] 5.4 `sensor.smart_geyser_pv_power` — pv_power_w (from pv object, None in v1)
- [x] 5.5 `sensor.smart_geyser_grid_power` — grid_power_w (from pv object, None in v1)
- [ ] 5.6 `sensor.smart_geyser_pv_opportunity_kwh_today` — deferred (v2: energy tracking)
- [ ] 5.7 `sensor.smart_geyser_pv_opportunity_fraction_30d` — deferred (v2)
- [ ] 5.8 `sensor.smart_geyser_grid_fraction_30d` — deferred (v2)
- [x] 5.9 `sensor.smart_geyser_next_predicted_use` — device class `timestamp`
- [x] 5.10 `sensor.smart_geyser_events_today` — state class `total_increasing`

## 6. Binary sensor entities (spec §8.2)

- [x] 6.1 `binary_sensor.smart_geyser_heating_active` — device class `running`
- [x] 6.2 `binary_sensor.smart_geyser_pump_active` — device class `running`
- [x] 6.3 `binary_sensor.smart_geyser_smart_stop_active` — device class `running`
- [x] 6.4 `binary_sensor.smart_geyser_preheat_active` — device class `running`
- [x] 6.5 `binary_sensor.smart_geyser_opportunity_active` — device class `running`

## 7. Number and switch controls (spec §8.3)

- [x] 7.1 `number.smart_geyser_setpoint` — range 40–75°C, step 0.5, calls POST /api/setpoint
- [ ] 7.2 `number.smart_geyser_soc_full_threshold` — deferred (v2: opportunity engine)
- [x] 7.3 `switch.smart_geyser_boost` — reads boost_until; turns on/off via API

## 8. Services

- [x] 8.1 `smart_geyser.boost` — accepts `duration_minutes`, calls `POST /api/boost`
- [x] 8.2 `smart_geyser.set_setpoint` — accepts `temp_c`, calls `POST /api/setpoint`
- [x] 8.3 Register services in `services.yaml` with field schemas

## 9. Lovelace dashboard

- [x] 9.1 Create `ha-integration/dashboards/smart_geyser.yaml`
- [x] 9.2 Card layout: tank temp gauge, status badges, schedule info, setpoint control, PV stats (conditional)
- [x] 9.3 Boost switch card included
- [x] 9.4 Dashboard uses only built-in HA Lovelace cards

## 10. Phase 6 wrap-up

- [x] 10.1 Run integration tests: `docker compose run --rm ha-test pytest` — 10/10 pass
- [ ] 10.2 Validate the manifest with HACS pre-check tooling (deferred — requires GitHub repo)
- [ ] 10.3 Tag commit `phase-6-complete`
- [x] 10.4 Update [CLAUDE.md](../CLAUDE.md) with integration decisions
- [x] 10.5 Phase 7 task file already exists
