# Smart Geyser Controller — Project Specification

**Version:** 0.5 — PV / Battery Integration + Opportunity Heating
**Status:** Work in Progress

---

## Changelog from v0.4

- Added `PVSystemProvider` trait — parallel to `GeyserProvider`, optional
- Added `OpportunityHeatingEngine` module to `smart-geyser-core`
- Added PV system type to `HeatingSystem` decision logic
- Smart-stop logic updated: yields to PV opportunity heating when conditions are met
- New PV provider implementations: `VictronProvider`, `GenericHAEntityProvider (PV)`
- New HA entities for PV state and opportunity heating status
- New config fields for PV thresholds

---

## 1. Core Principles (updated)

- **Smart brain is hardware-agnostic** — trait-based provider interface for both geyser and PV systems
- **System type is a first-class concept** — electric, solar+electric, heat pump all supported
- **The geyser is a thermal battery** — when the chemical battery is full and PV is generating, heat water with that free energy rather than exporting it cheaply or curtailing it
- **Smart-stop is about preventing waste, not preventing heating** — it yields when free electricity is available
- **HA integration is a thin client** — Python HA integration talks to Rust service via local REST
- **Runs as HA Add-on** — Docker container, standard add-on conventions
- **Standalone capable** — runs without HA via MQTT

---

## 2. PV System Model

### 2.1 PVSystemState

```rust
/// A snapshot of the PV and battery system at a point in time.
/// Only battery_soc_pct is required. All other fields are Optional.
#[derive(Debug, Clone)]
pub struct PVSystemState {
    pub timestamp:            chrono::DateTime<chrono::Utc>,

    // --- REQUIRED ---
    pub battery_soc_pct:      f32,       // 0.0 – 100.0

    // --- OPTIONAL: improves opportunity heating decisions ---
    pub pv_power_w:           Option<f32>,  // Current PV generation (W)
    pub grid_power_w:         Option<f32>,  // + = importing, − = exporting
    pub battery_power_w:      Option<f32>,  // + = charging, − = discharging
    pub load_power_w:         Option<f32>,  // Total home consumption (W)
    pub battery_capacity_kwh: Option<f32>,  // Total usable battery capacity
}
```

### 2.2 PVCapability

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PVCapability {
    PvPower,          // pv_power_w is populated
    GridPower,        // grid_power_w is populated — enables export detection
    BatteryPower,     // battery_power_w is populated
    LoadPower,        // load_power_w is populated — enables coverage calculation
    BatteryCapacity,  // battery_capacity_kwh is populated
}
```

### 2.3 PVSystemProvider Trait

```rust
#[async_trait]
pub trait PVSystemProvider: Send + Sync {
    async fn get_pv_state(&self) -> anyhow::Result<PVSystemState>;

    fn capabilities(&self) -> HashSet<PVCapability>;

    fn name(&self) -> &str;
}
```

This trait is **completely independent** of `GeyserProvider`. A system with only a battery SOC sensor (and nothing else) still satisfies the minimum contract for PV-integrated decisions.

### 2.4 Minimum PV Hardware Requirements

To enable PV opportunity heating a system must provide:

1. **Battery SOC** — at ≤ 5-minute intervals

That's it. Richer providers unlock better accuracy:

| PV Capability | Without it | With it |
|---|---|---|
| `GridPower` | Cannot detect active export | Triggers opportunity on active export (most reliable signal) |
| `PvPower` | Cannot calculate PV coverage | Triggers opportunity when PV > load + element (second signal) |
| `LoadPower` | Cannot calculate PV net surplus | Combined with `PvPower` gives precise excess calculation |
| `BatteryPower` | Cannot detect charging state | Confirms battery is not absorbing (charging rate ≈ 0) |
| `BatteryCapacity` | No energy context | Enables time-based "how long until full" estimates |

---

## 3. Opportunity Heating Engine

`smart-geyser-core` gains a new module: `opportunity_engine.rs`. It is separate from `decision_engine.rs` so they can be reasoned about independently. The coordinator calls both on each tick.

### 3.1 The Core Concept

```
When battery is full AND PV is generating surplus energy:
  → Heat water NOW with that free electricity
  → Instead of exporting to grid at a poor feed-in rate, or curtailing
  → The geyser becomes thermal storage for energy that would otherwise be lost
  → This applies EVEN IF smart-stop is active (since smart-stop prevents waste,
    not free-energy utilisation)
```

### 3.2 Trigger Conditions

Two independent trigger paths — either is sufficient:

**Path A — Export Detection** (preferred, most reliable):
```
battery_soc >= SOC_FULL_THRESHOLD   (default: 95%)
AND grid_power < −EXPORT_FLOOR_W    (default: −200 W — actively exporting meaningfully)
```

**Path B — PV Coverage** (fallback when GridPower not available):
```
battery_soc >= SOC_FULL_THRESHOLD
AND (pv_power − load_power) >= element_kw_as_watts × COVERAGE_RATIO  (default: 0.85)
```

**Path C — SOC-only** (minimum viable, no PV or grid data):
```
battery_soc >= SOC_FULL_THRESHOLD
AND current_time is within solar window
AND battery_power ≈ 0               (battery neither charging nor discharging significantly)
```
Path C is a weaker signal and has a higher temperature ceiling reduction applied.

### 3.3 Suppress Conditions

Even if a trigger fires, opportunity heating is suppressed when:

| Condition | Reason |
|---|---|
| `tank_temp >= OPPORTUNITY_MAX_TEMP` | Tank is already as full as safely useful |
| `heating_active == true` | Already heating — don't issue redundant commands |
| `outside_solar_window()` | No meaningful PV expected in remaining window |
| `solar_thermal_active AND tank_temp > SOLAR_THERMAL_CEILING` | Collector already doing the work at high temp — don't compound |
| `battery_soc < SOC_FULL_THRESHOLD` (dropped during cloud) | Opportunity has passed |

### 3.4 Solar Window

```rust
pub struct SolarWindow {
    pub latitude:  f32,   // For sunrise/sunset calculation
    pub longitude: f32,
    pub min_remaining_minutes: u32,   // Default: 45 — don't start if < this left in window
}

impl SolarWindow {
    /// Returns true if there is meaningful PV generation time remaining today.
    pub fn is_active(&self, now: DateTime<Utc>) -> bool { ... }

    pub fn minutes_remaining(&self, now: DateTime<Utc>) -> u32 { ... }
}
```

If latitude/longitude are not configured, solar window defaults to 07:00–17:00 local time.

### 3.5 Temperature Targets for Opportunity Heating

Opportunity heating uses a **separate temperature ceiling** from the normal scheduling setpoint. The intent is to store as much thermal energy as safely possible when free electricity is available.

```rust
pub struct OpportunityConfig {
    /// SOC % at which the battery is considered "full".
    /// Default: 95.0 — captures near-full states and avoids chasing 100%.
    pub soc_full_threshold:        f32,   // default: 95.0

    /// Minimum grid export (W, negative) to trigger Path A.
    pub export_floor_w:            f32,   // default: 200.0

    /// Fraction of element power that must be covered by PV surplus for Path B.
    pub pv_coverage_ratio:         f32,   // default: 0.85

    /// Maximum tank temperature during opportunity heating.
    /// Higher than the normal setpoint — geyser as thermal battery.
    /// Bounded by hardware safety: never exceed provider's max safe temp.
    pub opportunity_max_temp_c:    f32,   // default: 70.0

    /// Minimum duration to run once started (anti-cycling against cloud cover).
    pub min_run_minutes:           u32,   // default: 15

    /// Minimum SOC drop before cancelling an active opportunity session.
    /// Prevents stopping immediately on a brief cloud shadow.
    pub soc_hysteresis_pct:        f32,   // default: 3.0

    /// Whether opportunity heating overrides smart-stop.
    /// Default: true — smart-stop prevents waste; PV heating is not waste.
    pub override_smart_stop:       bool,  // default: true
}
```

### 3.6 Smart-Stop Interaction

This is the most important design decision in the PV integration:

```
Smart-stop exists to prevent: grid electricity heating water that will
just cool unused overnight — a pure waste of money and energy.

PV opportunity heating is: using electricity that would otherwise be
exported cheaply or curtailed — not a waste.

Therefore:
  smart_stop DOES block: scheduled pre-heat when no PV opportunity exists
  smart_stop DOES NOT block: opportunity heating when PV surplus is confirmed

This means a household can have their last shower at 20:00, smart-stop
engages as normal, but at 14:30 the same day (with battery full and PV
surplus) the geyser was already heated to 70°C — arriving at the shower
with more hot water at higher temperature than the 60°C setpoint alone
would have provided.
```

State machine:

```
SMART_STOP_ACTIVE:
  + schedule-driven heating:    BLOCKED
  + opportunity heating:        ALLOWED if PV conditions met AND tank < OPPORTUNITY_MAX_TEMP
```

### 3.7 Solar Thermal Interaction (SolarPumped systems)

When `HeatingSystem` is `SolarPumped` and `collector_temp` is available:

```rust
fn solar_thermal_doing_useful_work(state: &GeyserState, config: &OpportunityConfig) -> bool {
    match (state.collector_temp_c, state.pump_active) {
        (Some(collector), Some(true)) => {
            // Collector is hotter than tank — thermal gain is happening
            collector > state.tank_temp_c + 5.0
        },
        _ => false,
    }
}
```

If solar thermal is actively heating AND `tank_temp > SOLAR_THERMAL_CEILING` (default: 65°C), skip PV opportunity heating — the collector is already doing the work and the tank is approaching a useful limit. Don't compound thermal stress.

If solar thermal is active but tank is still below the ceiling, PV opportunity heating can proceed in parallel — both energy sources fill the tank together.

---

## 4. Updated Decision Engine Integration

`decision_engine.rs` and `opportunity_engine.rs` run as separate async tasks but share state via a `SharedEngineState` struct protected by an `Arc<RwLock<_>>`:

```rust
pub struct SharedEngineState {
    pub smart_stop_active:      bool,
    pub opportunity_active:     bool,   // PV opportunity session in progress
    pub last_opportunity_start: Option<DateTime<Utc>>,
    pub preheat_active:         bool,
    pub boost_until:            Option<DateTime<Utc>>,
}
```

Priority order for element control (highest wins):

```
1. BOOST          — manual override, always wins
2. FAULT          — hardware error, always blocks
3. OPPORTUNITY    — PV surplus, overrides smart-stop
4. PREHEAT        — scheduled pre-heat, blocked by smart-stop
5. IDLE           — no action
```

---

## 5. Updated Repository Structure

```
smart-geyser/
│
├── crates/
│   │
│   ├── smart-geyser-core/
│   │   └── src/
│   │       ├── provider.rs           # GeyserProvider trait (unchanged)
│   │       ├── pv_provider.rs        # PVSystemProvider trait (NEW)
│   │       ├── system.rs             # HeatingSystem enum (unchanged)
│   │       ├── event_detector.rs     # Use event detection (unchanged)
│   │       ├── pattern_store.rs      # Usage histogram (unchanged)
│   │       ├── decision_engine.rs    # Pre-heat + smart-stop (updated)
│   │       ├── opportunity_engine.rs # PV opportunity heating (NEW)
│   │       ├── solar_window.rs       # Sunrise/sunset calculation (NEW)
│   │       ├── heat_calc.rs          # Lead time calc (unchanged)
│   │       └── models.rs             # + OpportunityConfig, PVSystemState
│   │
│   ├── smart-geyser-providers/
│   │   └── src/
│   │       ├── geyserwala.rs         # GeyserProvider (unchanged)
│   │       ├── ha_entity.rs          # GeyserProvider (unchanged)
│   │       ├── mqtt_generic.rs       # GeyserProvider (unchanged)
│   │       │
│   │       ├── pv/                   # (NEW folder)
│   │       │   ├── mod.rs
│   │       │   ├── victron.rs        # VictronProvider (MQTT from Venus OS)
│   │       │   ├── sunsynk.rs        # SunsynkProvider (Sunsynk/Deye HA API)
│   │       │   ├── ha_entity_pv.rs   # GenericHAPVProvider (any HA entities)
│   │       │   └── mqtt_generic_pv.rs # GenericMqttPVProvider
│   │       └── ...
│   │
│   └── smart-geyser-service/
│       └── src/
│           ├── api/
│           │   ├── state.rs          # + pv_state in response
│           │   ├── status.rs         # + opportunity_active, pv fields
│           │   └── ...
│           └── scheduler.rs          # Runs both engines concurrently
```

---

## 6. PV Providers

### 6.1 VictronProvider

Connects to Venus OS (Cerbo GX, Venus GX, Ekrano GX, Raspberry Pi running Venus OS) via its local MQTT broker. The Victron MQTT topic structure follows the pattern `N/{portal_id}/system/0/{key}`.

```rust
pub struct VictronProviderConfig {
    pub broker_host: String,       // IP of Venus OS device
    pub broker_port: u16,          // Default: 1883
    pub portal_id:   String,       // Venus OS portal/system ID (from VRM)
    pub keepalive_interval_secs: u32,  // Default: 30 — required by Venus OS MQTT
}

// Topics consumed:
//   N/{id}/system/0/Dc/Battery/Soc           → battery_soc_pct
//   N/{id}/system/0/Dc/Pv/Power              → pv_power_w
//   N/{id}/system/0/Ac/Grid/L1/Power         → grid_power_w (single phase)
//   N/{id}/system/0/Dc/Battery/Power         → battery_power_w
//   N/{id}/system/0/Ac/Consumption/L1/Power  → load_power_w
```

> **Venus OS MQTT keep-alive:** The Venus OS MQTT broker requires a keep-alive message every 30 seconds (`R/{portal_id}/system/0/Serial → {}`), otherwise it stops publishing. The `VictronProvider` sends this automatically.

```rust
impl PVSystemProvider for VictronProvider {
    fn capabilities(&self) -> HashSet<PVCapability> {
        HashSet::from([
            PVCapability::PvPower,
            PVCapability::GridPower,
            PVCapability::BatteryPower,
            PVCapability::LoadPower,
        ])
    }

    fn name(&self) -> &str { "Victron Venus OS" }
}
```

### 6.2 SunsynkProvider

Sunsynk (and rebranded Deye) inverters are very common in South Africa. Integration via the `sunsynk` HA integration or direct API.

```rust
pub struct SunsynkProviderConfig {
    pub ha_base_url: String,
    pub ha_token:    String,
    // Entity IDs from the sunsynk HA integration
    pub soc_entity:      String,   // sensor.battery_soc
    pub pv_power_entity: Option<String>,
    pub grid_power_entity: Option<String>,
    pub battery_power_entity: Option<String>,
    pub load_power_entity: Option<String>,
}
```

### 6.3 GenericHAPVProvider

Maps any existing HA entities to `PVSystemState`. Covers: Goodwe, Fronius, SMA, Sungrow, Fox ESS, Growatt, EG4, custom ESPHome inverter monitors, Shelly EM energy monitors, and any other inverter that is already in HA.

```rust
pub struct GenericHAPVConfig {
    pub ha_base_url:  String,
    pub ha_token:     String,

    // REQUIRED
    pub soc_entity:             String,   // Any sensor in 0–100 range

    // OPTIONAL
    pub pv_power_entity:        Option<String>,
    pub grid_power_entity:      Option<String>,   // + import, − export convention
    pub battery_power_entity:   Option<String>,
    pub load_power_entity:      Option<String>,
    pub battery_capacity_entity: Option<String>,  // kWh

    // Power sign convention (some inverters report export as positive)
    pub grid_export_is_positive: bool,  // default: false (negative = export)
}
```

### 6.4 GenericMqttPVProvider

```rust
pub struct MqttPVConfig {
    pub broker:              String,
    pub port:                u16,
    pub soc_topic:           String,
    pub pv_power_topic:      Option<String>,
    pub grid_power_topic:    Option<String>,
    pub battery_power_topic: Option<String>,
    pub load_power_topic:    Option<String>,
    pub grid_export_is_positive: bool,
}
```

### 6.5 PV Provider Summary

| Provider | Status | Inverters / Systems |
|---|---|---|
| `VictronProvider` | v1 | Victron MultiPlus, Quattro, EasySolar, any Venus OS system |
| `SunsynkProvider` | v1 | Sunsynk, Deye (very common in South Africa) |
| `GenericHAPVProvider` | v1 | Any inverter already in HA: Goodwe, Fronius, SMA, Sungrow, Growatt, Fox ESS, Shelly EM, custom ESPHome |
| `GenericMqttPVProvider` | v1 | Custom firmware, non-HA setups |
| `ESPHomeNativePVProvider` | future | Direct ESPHome API for custom energy monitors |

---

## 7. Updated Service API

### 7.1 New / Updated Endpoints

```
GET  /api/pv-state              Current PVSystemState (latest poll)
GET  /api/status                (updated — includes PV + opportunity fields)
GET  /api/opportunity-log       Recent opportunity heating sessions
```

### 7.2 Updated Status Response

```json
{
  "system_type": "solar_pumped",
  "provider": "Geyserwala Connect",

  "tank_temp_c": 68.4,
  "collector_temp_c": 81.2,
  "pump_active": true,
  "heating_active": true,
  "smart_stop_active": false,
  "preheat_active": false,

  "pv": {
    "provider": "Victron Venus OS",
    "battery_soc_pct": 97.3,
    "pv_power_w": 3420,
    "grid_power_w": -1180,
    "battery_power_w": 12,
    "load_power_w": 2228,
    "opportunity_active": true,
    "opportunity_reason": "battery_full_exporting",
    "opportunity_target_temp_c": 70.0,
    "solar_window_minutes_remaining": 142
  },

  "next_predicted_use": "2026-04-28T17:30:00Z",
  "preheat_starts_at": "2026-04-28T17:08:00Z",
  "events_today": 1,

  "energy_today": {
    "solar_thermal_kwh": 2.1,
    "pv_opportunity_kwh": 1.4,
    "grid_electric_kwh": 0.0,
    "total_kwh": 3.5
  },

  "energy_30d": {
    "solar_thermal_fraction_pct": 58.2,
    "pv_opportunity_fraction_pct": 28.7,
    "grid_fraction_pct": 13.1
  }
}
```

Note the three-way energy split: solar thermal, PV opportunity, and grid. This gives the user a clear picture of where their hot water energy comes from.

---

## 8. Updated HA Entities

### 8.1 New PV Sensors

| Entity | Description |
|---|---|
| `sensor.smart_geyser_battery_soc` | Current battery SOC % |
| `sensor.smart_geyser_pv_power` | Current PV generation (W) |
| `sensor.smart_geyser_grid_power` | Grid import/export (W) |
| `sensor.smart_geyser_pv_opportunity_kwh_today` | kWh heated by PV diversion today |
| `sensor.smart_geyser_pv_opportunity_fraction_30d` | % of hot water energy from PV diversion (30-day) |
| `sensor.smart_geyser_grid_fraction_30d` | % of hot water energy from grid (30-day) |

### 8.2 New Binary Sensors

| Entity | Description |
|---|---|
| `binary_sensor.smart_geyser_opportunity_active` | PV opportunity heating session in progress |

### 8.3 New Controls

| Entity | Description |
|---|---|
| `number.smart_geyser_opportunity_max_temp` | Temperature ceiling for opportunity heating (default: 70°C) |
| `number.smart_geyser_soc_full_threshold` | SOC % to consider battery "full" (default: 95) |
| `switch.smart_geyser_opportunity_enabled` | Enable/disable PV opportunity heating |

---

## 9. Updated EngineConfig

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    // --- System ---
    pub system: HeatingSystem,

    // --- Normal scheduling ---
    pub setpoint_c:              f32,    // default: 60.0
    pub hysteresis_c:            f32,    // default: 4.0
    pub preheat_threshold:       f32,    // default: 0.40
    pub late_use_threshold:      f32,    // default: 0.15
    pub cutoff_buffer_min:       u32,    // default: 30
    pub safety_margin_min:       u32,    // default: 20
    pub decay_factor:            f32,    // default: 0.995
    pub legionella_interval_days: u32,   // default: 7

    // --- PV opportunity (optional — None disables PV integration) ---
    pub opportunity: Option<OpportunityConfig>,

    // --- Solar window (used by opportunity engine) ---
    pub solar_window: Option<SolarWindow>,
}
```

---

## 10. Usage Scenarios

### 10.1 Electric-only geyser, no PV

No `PVSystemProvider` configured. `opportunity` is `None`.
Behaviour: fully pattern-based scheduling + smart-stop. Pure learning controller.

### 10.2 Solar pumped geyser, no PV battery

`HeatingSystem::SolarPumped`, no `PVSystemProvider`.
Behaviour: solar thermal pump controlled by Geyserwala. Electric element only fires based on pattern scheduling. Smart-stop active.

### 10.3 Solar pumped geyser + Victron battery/PV

`HeatingSystem::SolarPumped` + `VictronProvider`.
Behaviour: Solar thermal heats passively. When battery hits 95% SOC and export is detected, element fires to heat water to 70°C. Smart-stop yields to PV opportunity. Pattern learning still schedules pre-heat for days when PV is insufficient.

### 10.4 Electric-only geyser + Sunsynk PV + battery

`HeatingSystem::ElectricOnly` + `SunsynkProvider`.
Behaviour: no solar thermal at all. Pattern scheduling pre-heats from grid when needed. On good solar days, PV opportunity tops up the tank to 70°C before the battery fills completely (SOC at 95% with export detected). Net grid consumption from water heating approaches zero on sunny days.

### 10.5 Heat pump + Victron battery/PV

`HeatingSystem::HeatPump` + `VictronProvider`.
Behaviour: heat pump is the primary heating device. Lead time calculated using `cop_nominal` (or `live_cop` if available). PV opportunity heating works the same way — but since the heat pump has a COP of ~3.5, each kWh of PV electricity diverted yields ~3.5 kWh of thermal energy. The thermal storage value per unit of exported PV is dramatically better than with a resistive element.

---

## 11. Key Design Decisions (rationale)

### Why 95% SOC threshold instead of 100%?

Battery SoC sensors typically have ±2–3% accuracy. Many BMS systems also actively manage around 95–98% during float charging. Waiting for exactly 100% means the opportunity window is often missed entirely. 95% is a conservative threshold that reliably indicates "no meaningful additional battery charging is happening."

### Why not discharge the battery to heat water?

The system never deliberately discharges the battery for water heating. `battery_power_w` is used to detect that the battery is at rest (not charging significantly), confirming that PV surplus is genuinely available. If the battery is discharging (grid outage, evening), opportunity heating is blocked.

### Why does smart-stop yield to opportunity heating?

Smart-stop prevents grid energy from heating water that will just cool overnight. That reasoning doesn't apply to PV surplus: if the water is heated to 70°C at 14:00 using exported PV electricity, it retains the majority of that energy by morning and reduces or eliminates the need for grid pre-heating the next day. The thermal energy is not wasted — it shifts the energy source of the next day's hot water from grid to PV.

### Why a separate temperature ceiling (70°C) for opportunity heating?

The normal 60°C setpoint is chosen for comfort and efficiency in daily use. Opportunity heating goes to 70°C because: (a) it's treating the tank as thermal storage, not a daily-use target; (b) the extra 10°C represents ~15% more stored thermal energy in a 150 L tank (~9 kWh vs ~7.8 kWh from 20°C); (c) it is still well within the Geyserwala's safe operating range (max 75°C normal, 90°C hardware cutout). Scalding risk at the tap is managed by the existing tempering valve (required by building code on any geyser system).

### Why a minimum run time (anti-cycling)?

Residential PV output fluctuates with partial cloud cover. Without a minimum run time, the element could start and stop every 3–5 minutes as a cloud passes over — this is hard on the relay and element, reduces efficiency, and provides little useful heat. A 15-minute minimum ensures each session is worthwhile and the hardware is protected.

---

## 12. Open Questions (updated)

| # | Question | Priority |
|---|---|---|
| 1 | GeyserWise model installed? | High |
| 2 | Geyserwala Connect fitted? | High |
| 3 | Tank volume and element kW? | High |
| 4 | Pump voltage (12V DC or 220V AC)? | High |
| 5 | PV / battery system make and model? (Victron, Sunsynk, other?) | High |
| 6 | Is tempering valve fitted? (Required if opportunity max temp > 60°C) | Medium |
| 7 | Publish `smart-geyser-core` to crates.io as standalone? | Medium |
| 8 | South Africa load-shedding schedule as a first-class concept? (Eskom EskomSePush API) | Medium |
| 9 | Time-of-use tariff awareness? | Low |
| 10 | Heat pump COP degradation model at low ambient temperatures? | Low |

---

## 13. Development Phases (updated)

### Phase 1 — Core Library
- `provider.rs`, `pv_provider.rs`, `system.rs`, `models.rs`, `heat_calc.rs`
- All unit tests for heat calculations and system type variants

### Phase 2 — Event Detection & Pattern Store
- `event_detector.rs`, `pattern_store.rs`, `decision_engine.rs` skeleton

### Phase 3 — Opportunity Engine
- `opportunity_engine.rs`, `solar_window.rs`
- Unit tests: synthetic PV state stream → verify opportunity triggers and suppression
- Test smart-stop override scenarios explicitly

### Phase 4 — Providers
- Geyser: `GeyserwalaProvider`, `GenericHAEntityProvider`, `GenericMqttProvider`
- PV: `VictronProvider`, `SunsynkProvider`, `GenericHAPVProvider`, `GenericMqttPVProvider`

### Phase 5 — Service & Add-on
- `smart-geyser-service`: axum API, scheduler, config
- Docker build + HA add-on manifest

### Phase 6 — HA Integration
- Python thin client, config flow with provider selection
- Full dashboard

### Phase 7 — Polish & Release
- HACS listing, hardware guides, provider guide
- Load-shedding integration (South Africa)

---

*Last updated: 2026-04-28*
