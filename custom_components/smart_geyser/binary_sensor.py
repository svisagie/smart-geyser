"""Binary sensor entities for Smart Geyser Controller."""
from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from homeassistant.components.binary_sensor import (
    BinarySensorDeviceClass,
    BinarySensorEntity,
    BinarySensorEntityDescription,
)
from homeassistant.config_entries import ConfigEntry
from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddEntitiesCallback
from homeassistant.helpers.update_coordinator import CoordinatorEntity

from .const import DOMAIN
from .coordinator import SmartGeyserCoordinator


@dataclass(frozen=True, kw_only=True)
class SmartGeyserBinarySensorDescription(BinarySensorEntityDescription):
    value_fn: Any = None


BINARY_SENSOR_DESCRIPTIONS: tuple[SmartGeyserBinarySensorDescription, ...] = (
    SmartGeyserBinarySensorDescription(
        key="heating_active",
        name="Heating Active",
        device_class=BinarySensorDeviceClass.RUNNING,
        value_fn=lambda s: s.heating_active,
    ),
    SmartGeyserBinarySensorDescription(
        key="pump_active",
        name="Pump Active",
        device_class=BinarySensorDeviceClass.RUNNING,
        value_fn=lambda s: s.pump_active,
    ),
    SmartGeyserBinarySensorDescription(
        key="smart_stop_active",
        name="Smart Stop Active",
        device_class=BinarySensorDeviceClass.RUNNING,
        value_fn=lambda s: s.smart_stop_active,
    ),
    SmartGeyserBinarySensorDescription(
        key="preheat_active",
        name="Pre-heat Active",
        device_class=BinarySensorDeviceClass.RUNNING,
        value_fn=lambda s: s.preheat_active,
    ),
    SmartGeyserBinarySensorDescription(
        key="opportunity_active",
        name="PV Opportunity Heating",
        device_class=BinarySensorDeviceClass.RUNNING,
        icon="mdi:solar-power-variant",
        value_fn=lambda s: bool(s.pv and s.pv.get("opportunity_active")),
    ),
)


async def async_setup_entry(
    hass: HomeAssistant,
    entry: ConfigEntry,
    async_add_entities: AddEntitiesCallback,
) -> None:
    coordinator: SmartGeyserCoordinator = hass.data[DOMAIN][entry.entry_id]
    async_add_entities(
        SmartGeyserBinarySensor(coordinator, entry, desc)
        for desc in BINARY_SENSOR_DESCRIPTIONS
    )


class SmartGeyserBinarySensor(
    CoordinatorEntity[SmartGeyserCoordinator], BinarySensorEntity
):
    entity_description: SmartGeyserBinarySensorDescription
    _attr_has_entity_name = True

    def __init__(
        self,
        coordinator: SmartGeyserCoordinator,
        entry: ConfigEntry,
        description: SmartGeyserBinarySensorDescription,
    ) -> None:
        super().__init__(coordinator)
        self.entity_description = description
        self._attr_unique_id = f"{entry.entry_id}_{description.key}"
        self._attr_device_info = {
            "identifiers": {(DOMAIN, entry.entry_id)},
            "name": "Smart Geyser Controller",
        }

    @property
    def is_on(self) -> bool | None:
        if self.coordinator.data is None:
            return None
        try:
            result = self.entity_description.value_fn(self.coordinator.data)
            return bool(result) if result is not None else None
        except (AttributeError, KeyError, TypeError):
            return None

    @property
    def available(self) -> bool:
        return self.coordinator.last_update_success and self.coordinator.data is not None
