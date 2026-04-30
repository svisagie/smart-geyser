# Solar-Thermal Pumped Geyser — No PV Battery (Scenario §10.2)

A Geyserwala Connect manages both the solar-thermal pump and the electric element. The electric element only fires when solar collection is insufficient.

## Prerequisites

- **Geyserwala Connect** installed with a solar collector temperature probe
- Solar pump wired to the Geyserwala's pump relay
- Collector temperature probe installed and connected

## Hardware setup

1. Install the Geyserwala with the collector probe (typically PT1000 sensor) connected to the "Collector" terminal.
2. Wire the solar pump to the Geyserwala's pump output relay.
3. Enable **External Demand** control in the Geyserwala app.

## Configuration

```toml
[geyser]
type = "geyserwala"
base_url = "http://192.168.1.50"
element_kw = 3.0
tank_volume_l = 150.0

[engine]
setpoint_c = 60.0
```

The `system_type` is automatically detected as `solar_pumped` from the Geyserwala provider.

## Expected HA entities

All entities from the electric-only scenario, plus:

| Entity | Description |
|---|---|
| `sensor.smart_geyser_collector_temperature` | Solar collector temperature |
| `binary_sensor.smart_geyser_pump_active` | Solar pump running |

## Behaviour

- The Geyserwala manages the pump automatically (differential temperature control).
- The smart-geyser service controls the **electric element** only.
- Smart-stop suppresses the element when patterns show no use expected soon, but the pump continues to collect solar energy regardless.

## Verify it's working

- [ ] `sensor.smart_geyser_collector_temperature` shows temperature (higher than tank on a sunny day)
- [ ] `binary_sensor.smart_geyser_pump_active` turns on when sun is available
- [ ] On a cloudy evening, the electric element pre-heats before your typical shower time
