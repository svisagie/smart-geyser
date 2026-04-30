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
from homeassistant.const import UnitOfEnergy, UnitOfPower, UnitOfTemperature
from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddEntitiesCallback
from homeassistant.helpers.update_coordinator import CoordinatorEntity

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


async def async_setup_entry(
    hass: HomeAssistant,
    entry: ConfigEntry,
    async_add_entities: AddEntitiesCallback,
) -> None:
    coordinator: SmartGeyserCoordinator = hass.data[DOMAIN][entry.entry_id]
    async_add_entities(
        SmartGeyserSensor(coordinator, entry, desc) for desc in SENSOR_DESCRIPTIONS
    )


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
