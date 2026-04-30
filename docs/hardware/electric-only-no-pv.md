# Electric-Only Geyser — No PV (Scenario §10.1)

This is the simplest configuration: a standard resistive element geyser controlled by a Geyserwala Connect device. No solar thermal, no PV battery.

## Prerequisites

- **Geyserwala Connect** installed on your geyser
- Geyserwala Connect reachable on your local network
- Home Assistant (optional — the service works standalone)

## Hardware setup

1. Install the Geyserwala Connect according to its manual. The device wires to the geyser's element contactor.
2. Connect the Geyserwala to your Wi-Fi via the Geyserwala app.
3. Note the device IP address (shown in the app under "Device Info").
4. In the Geyserwala app, enable **External Demand** control (Settings → Advanced → External Demand).

## Configuration

```toml
[geyser]
type = "geyserwala"
base_url = "http://192.168.1.50"   # your device IP
element_kw = 3.0                   # your element rating
tank_volume_l = 150.0              # your tank size

[engine]
setpoint_c = 60.0
```

## Expected HA entities

| Entity | Description |
|---|---|
| `sensor.smart_geyser_tank_temperature` | Current tank temperature |
| `binary_sensor.smart_geyser_heating_active` | Element on/off |
| `binary_sensor.smart_geyser_smart_stop_active` | Smart-stop engaged |
| `binary_sensor.smart_geyser_preheat_active` | Pre-heat scheduled |
| `sensor.smart_geyser_next_predicted_use` | Predicted next hot-water use |
| `switch.smart_geyser_manual_boost` | Trigger 60-minute boost |
| `number.smart_geyser_setpoint_temperature` | Target temperature |

## Verify it's working

- [ ] `sensor.smart_geyser_tank_temperature` shows a plausible temperature
- [ ] Turn on Manual Boost → geyser heats → `binary_sensor.smart_geyser_heating_active` turns on
- [ ] Turn off Manual Boost → element stops
- [ ] After a few days of normal use, `sensor.smart_geyser_next_predicted_use` shows a future time
