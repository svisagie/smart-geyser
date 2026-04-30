# Smart Geyser Controller — Add-on Documentation

## Overview

This add-on controls your electric geyser (hot-water heater) using pattern learning. It observes when you typically use hot water and pre-heats the tank just before you need it — avoiding unnecessary heating at other times.

## Requirements

- A **Geyserwala Connect** device installed on your geyser
- The Geyserwala Connect must be accessible on your local network
- Note the device's IP address (shown in the Geyserwala app)

## Installation

1. In Home Assistant, go to **Settings → Add-ons → Add-on Store**
2. Add the repository URL for this add-on
3. Install **Smart Geyser Controller**
4. Configure the add-on (see below)
5. Start the add-on

## Configuration

| Option | Default | Description |
|---|---|---|
| `geyser.base_url` | *(required)* | URL of your Geyserwala Connect device, e.g. `http://192.168.1.50` |
| `geyser.token` | *(empty)* | API token if authentication is enabled on the device |
| `geyser.element_kw` | `3.0` | Element nameplate power in kilowatts |
| `geyser.tank_volume_l` | `150.0` | Tank volume in litres |
| `engine.setpoint_c` | `60.0` | Target water temperature (°C) |
| `engine.legionella_interval_days` | `7` | Force 65 °C heat every N days to kill legionella |
| `tick_interval_secs` | `60` | How often to poll the geyser and make decisions |
| `data_dir` | `/data` | Where to store the learned usage pattern (persisted between restarts) |

## First Run

On first start the controller has no usage data, so it will operate conservatively — keeping the tank at the setpoint temperature and learning your patterns. After a few days of normal use it will start pre-heating the tank ahead of your typical shower/bath times.

## REST API

The add-on exposes a REST API on port 8080:

| Endpoint | Method | Description |
|---|---|---|
| `/api/status` | GET | Current geyser state, engine status, and predictions |
| `/api/boost` | POST | Force heating for `duration_minutes` (1–480) |
| `/api/boost` | DELETE | Cancel active boost |
| `/api/setpoint` | POST | Change target temperature `temp_c` (40–75 °C) |
| `/api/opportunity-log` | GET | Recent PV opportunity heating sessions (v2) |
| `/api/pv-state` | GET | Current PV system state (v2, returns 404 in v1) |

Example — trigger a 60-minute boost:
```bash
curl -X POST http://homeassistant.local:8080/api/boost \
     -H "Content-Type: application/json" \
     -d '{"duration_minutes": 60}'
```

## Troubleshooting

- **Geyser not responding**: Check the `base_url` is correct and the Geyserwala Connect is reachable from the HA host.
- **Pattern not learning**: Ensure `data_dir` is set and the volume is writable.
- **Element not turning on**: Verify `external-demand` control is enabled in the Geyserwala app.
