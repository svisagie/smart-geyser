"""Number entities for Smart Geyser Controller."""
from __future__ import annotations

from homeassistant.components.number import (
    NumberDeviceClass,
    NumberEntity,
    NumberEntityDescription,
    NumberMode,
)
from homeassistant.config_entries import ConfigEntry
from homeassistant.const import UnitOfTemperature
from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddEntitiesCallback
from homeassistant.helpers.update_coordinator import CoordinatorEntity

from .const import DOMAIN
from .coordinator import SmartGeyserCoordinator


async def async_setup_entry(
    hass: HomeAssistant,
    entry: ConfigEntry,
    async_add_entities: AddEntitiesCallback,
) -> None:
    coordinator: SmartGeyserCoordinator = hass.data[DOMAIN][entry.entry_id]
    async_add_entities([SmartGeyserSetpoint(coordinator, entry)])


class SmartGeyserSetpoint(CoordinatorEntity[SmartGeyserCoordinator], NumberEntity):
    """Controls the geyser heating setpoint temperature."""

    _attr_has_entity_name = True
    _attr_name = "Setpoint Temperature"
    _attr_device_class = NumberDeviceClass.TEMPERATURE
    _attr_native_unit_of_measurement = UnitOfTemperature.CELSIUS
    _attr_native_min_value = 40.0
    _attr_native_max_value = 75.0
    _attr_native_step = 0.5
    _attr_mode = NumberMode.BOX

    def __init__(
        self,
        coordinator: SmartGeyserCoordinator,
        entry: ConfigEntry,
    ) -> None:
        super().__init__(coordinator)
        self._attr_unique_id = f"{entry.entry_id}_setpoint"
        self._attr_device_info = {
            "identifiers": {(DOMAIN, entry.entry_id)},
            "name": "Smart Geyser Controller",
        }
        self._entry = entry

    @property
    def native_value(self) -> float | None:
        # The service doesn't echo the setpoint in the status response yet;
        # return None to let HA show the last set value.
        return None

    async def async_set_native_value(self, value: float) -> None:
        await self.coordinator.client.post_setpoint(value)
