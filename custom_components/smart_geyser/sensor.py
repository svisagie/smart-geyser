"""Sensor entities for Smart Geyser Controller."""
from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime
from typing import Any

from homeassistant.components.sensor import (
    SensorDeviceClass,
    SensorEntity,
    SensorEntityDescription,
    SensorStateClass,
)
from homeassistant.config_entries import ConfigEntry
from homeassistant.const import EntityCategory, UnitOfEnergy, UnitOfPower, UnitOfTemperature, UnitOfTime
from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddEntitiesCallback
from homeassistant.helpers.update_coordinator import CoordinatorEntity

from .api_client import GeyserConfig
from .const import DOMAIN
from .coordinator import SmartGeyserCoordinator


@dataclass(frozen=True, kw_only=True)
class SmartGeyserSensorDescription(SensorEntityDescription):
    """Extends SensorEntityDescription with a value extractor."""

    value_fn: Any = None  # callable(GeyserStatus) -> value


SENSOR_DESCRIPTIONS: tuple[SmartGeyserSensorDescription, ...] = (
    SmartGeyserSensorDescription(
        key="tank_temp",
        name="Tank Temperature",
        device_class=SensorDeviceClass.TEMPERATURE,
        native_unit_of_measurement=UnitOfTemperature.CELSIUS,
        state_class=SensorStateClass.MEASUREMENT,
        value_fn=lambda s: s.tank_temp_c,
    ),
    SmartGeyserSensorDescription(
        key="collector_temp",
        name="Collector Temperature",
        device_class=SensorDeviceClass.TEMPERATURE,
        native_unit_of_measurement=UnitOfTemperature.CELSIUS,
        state_class=SensorStateClass.MEASUREMENT,
        value_fn=lambda s: s.collector_temp_c,
    ),
    SmartGeyserSensorDescription(
        key="next_predicted_use",
        name="Next Predicted Use",
        device_class=SensorDeviceClass.TIMESTAMP,
        value_fn=lambda s: s.next_predicted_use,
    ),
    SmartGeyserSensorDescription(
        key="events_today",
        name="Hot-Water Events Today",
        state_class=SensorStateClass.TOTAL_INCREASING,
        value_fn=lambda s: s.events_today,
    ),
    # PV sensors — unavailable in v1 (pv is None), become available in v2
    SmartGeyserSensorDescription(
        key="battery_soc",
        name="Battery SOC",
        device_class=SensorDeviceClass.BATTERY,
        native_unit_of_measurement="%",
        state_class=SensorStateClass.MEASUREMENT,
        value_fn=lambda s: s.pv["battery_soc_pct"] if s.pv else None,
    ),
    SmartGeyserSensorDescription(
        key="pv_power",
        name="PV Power",
        device_class=SensorDeviceClass.POWER,
        native_unit_of_measurement=UnitOfPower.WATT,
        state_class=SensorStateClass.MEASUREMENT,
        value_fn=lambda s: s.pv.get("pv_power_w") if s.pv else None,
    ),
    SmartGeyserSensorDescription(
        key="grid_power",
        name="Grid Power",
        device_class=SensorDeviceClass.POWER,
        native_unit_of_measurement=UnitOfPower.WATT,
        state_class=SensorStateClass.MEASUREMENT,
        value_fn=lambda s: s.pv.get("grid_power_w") if s.pv else None,
    ),
)


@dataclass(frozen=True, kw_only=True)
class SmartGeyserConfigSensorDescription(SensorEntityDescription):
    """Extends SensorEntityDescription with a GeyserConfig value extractor."""

    value_fn: Any = None  # callable(GeyserConfig) -> value


CONFIG_SENSOR_DESCRIPTIONS: tuple[SmartGeyserConfigSensorDescription, ...] = (
    SmartGeyserConfigSensorDescription(
        key="cfg_setpoint",
        name="Configured Setpoint",
        device_class=SensorDeviceClass.TEMPERATURE,
        native_unit_of_measurement=UnitOfTemperature.CELSIUS,
        entity_category=EntityCategory.DIAGNOSTIC,
        value_fn=lambda c: c.setpoint_c,
    ),
    SmartGeyserConfigSensorDescription(
        key="cfg_hysteresis",
        name="Hysteresis Band",
        device_class=SensorDeviceClass.TEMPERATURE,
        native_unit_of_measurement=UnitOfTemperature.CELSIUS,
        entity_category=EntityCategory.DIAGNOSTIC,
        value_fn=lambda c: c.hysteresis_c,
    ),
    SmartGeyserConfigSensorDescription(
        key="cfg_preheat_threshold",
        name="Preheat Probability Threshold",
        native_unit_of_measurement="%",
        entity_category=EntityCategory.DIAGNOSTIC,
        value_fn=lambda c: round(c.preheat_threshold * 100, 1),
    ),
    SmartGeyserConfigSensorDescription(
        key="cfg_late_use_threshold",
        name="Late-Use Probability Threshold",
        native_unit_of_measurement="%",
        entity_category=EntityCategory.DIAGNOSTIC,
        value_fn=lambda c: round(c.late_use_threshold * 100, 1),
    ),
    SmartGeyserConfigSensorDescription(
        key="cfg_legionella_interval",
        name="Legionella Cycle Interval",
        native_unit_of_measurement="d",
        entity_category=EntityCategory.DIAGNOSTIC,
        value_fn=lambda c: c.legionella_interval_days,
    ),
    SmartGeyserConfigSensorDescription(
        key="cfg_tick_interval",
        name="Poll Interval",
        device_class=SensorDeviceClass.DURATION,
        native_unit_of_measurement=UnitOfTime.SECONDS,
        entity_category=EntityCategory.DIAGNOSTIC,
        value_fn=lambda c: c.tick_interval_secs,
    ),
)


async def async_setup_entry(
    hass: HomeAssistant,
    entry: ConfigEntry,
    async_add_entities: AddEntitiesCallback,
) -> None:
    coordinator: SmartGeyserCoordinator = hass.data[DOMAIN][entry.entry_id]
    entities: list[SensorEntity] = [
        SmartGeyserSensor(coordinator, entry, desc) for desc in SENSOR_DESCRIPTIONS
    ]
    if coordinator.addon_config is not None:
        entities += [
            SmartGeyserConfigSensor(coordinator, entry, desc)
            for desc in CONFIG_SENSOR_DESCRIPTIONS
        ]
    async_add_entities(entities)


class SmartGeyserSensor(CoordinatorEntity[SmartGeyserCoordinator], SensorEntity):
    """A sensor backed by the coordinator snapshot."""

    entity_description: SmartGeyserSensorDescription
    _attr_has_entity_name = True

    def __init__(
        self,
        coordinator: SmartGeyserCoordinator,
        entry: ConfigEntry,
        description: SmartGeyserSensorDescription,
    ) -> None:
        super().__init__(coordinator)
        self.entity_description = description
        self._attr_unique_id = f"{entry.entry_id}_{description.key}"
        self._attr_device_info = {
            "identifiers": {(DOMAIN, entry.entry_id)},
            "name": "Smart Geyser Controller",
            "manufacturer": coordinator.data.provider if coordinator.data else "Smart Geyser",
        }

    @property
    def native_value(self) -> Any:
        if self.coordinator.data is None:
            return None
        try:
            return self.entity_description.value_fn(self.coordinator.data)
        except (AttributeError, KeyError, TypeError):
            return None

    @property
    def available(self) -> bool:
        return self.coordinator.last_update_success and self.coordinator.data is not None


class SmartGeyserConfigSensor(CoordinatorEntity[SmartGeyserCoordinator], SensorEntity):
    """A diagnostic sensor backed by the static add-on configuration."""

    entity_description: SmartGeyserConfigSensorDescription
    _attr_has_entity_name = True

    def __init__(
        self,
        coordinator: SmartGeyserCoordinator,
        entry: ConfigEntry,
        description: SmartGeyserConfigSensorDescription,
    ) -> None:
        super().__init__(coordinator)
        self.entity_description = description
        self._attr_unique_id = f"{entry.entry_id}_{description.key}"
        self._attr_device_info = {
            "identifiers": {(DOMAIN, entry.entry_id)},
            "name": "Smart Geyser Controller",
        }

    @property
    def native_value(self) -> Any:
        cfg: GeyserConfig | None = self.coordinator.addon_config
        if cfg is None:
            return None
        try:
            return self.entity_description.value_fn(cfg)
        except (AttributeError, KeyError, TypeError):
            return None

    @property
    def available(self) -> bool:
        return self.coordinator.addon_config is not None
