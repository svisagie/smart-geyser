# Electric Geyser + Sunsynk/Deye PV Battery (Scenario §10.4)

This is the most common South African setup: electric geyser + Sunsynk (or Deye) hybrid inverter with a lithium battery.

> **Note:** Sunsynk PV provider is deferred to v2. This guide describes the planned configuration.

## Prerequisites

- Geyserwala Connect on the geyser
- Sunsynk or Deye hybrid inverter
- HA Sunsynk integration (`hacs.io/sunsynk`) providing the required entities

## Planned v2 configuration

```toml
[geyser]
type = "geyserwala"
base_url = "http://192.168.1.50"

[pv]
type = "sunsynk"
battery_soc_entity = "sensor.sunsynk_battery_soc"
pv_power_entity = "sensor.sunsynk_pv_power"
grid_power_entity = "sensor.sunsynk_grid_power"
load_power_entity = "sensor.sunsynk_load_power"
# Sign convention: positive = export (verify for your firmware version)
grid_export_is_positive = true

[engine]
setpoint_c = 60.0
```

## Tuning

- **`soc_full_threshold_pct`** (default: 95): The SOC % above which the battery is considered "full" and surplus PV can heat the geyser. Lower this if your battery rarely reaches 95%.
- **`opportunity_max_temp_c`** (default: 70): Maximum temperature during PV heating. The geyser as a thermal battery — cap this to protect the element and tempering valve.

## Verify it's working

- [ ] Battery SOC entity shows correct values in HA
- [ ] On a sunny day with full battery, `binary_sensor.smart_geyser_opportunity_active` turns on
- [ ] Tank reaches `opportunity_max_temp_c` then opportunity heating stops
- [ ] On a cloudy day, normal schedule-based heating fires as usual
