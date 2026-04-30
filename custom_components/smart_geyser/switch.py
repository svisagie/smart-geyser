"""Switch entities for Smart Geyser Controller (manual boost)."""
from __future__ import annotations

from homeassistant.components.switch import SwitchDeviceClass, SwitchEntity
from homeassistant.config_entries import ConfigEntry
from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddEntitiesCallback
from homeassistant.helpers.update_coordinator import CoordinatorEntity
from homeassistant.util.dt import utcnow

from .const import DOMAIN
from .coordinator import SmartGeyserCoordinator

DEFAULT_BOOST_MINUTES = 60


async def async_setup_entry(
    hass: HomeAssistant,
    entry: ConfigEntry,
    async_add_entities: AddEntitiesCallback,
) -> None:
    coordinator: SmartGeyserCoordinator = hass.data[DOMAIN][entry.entry_id]
    async_add_entities([
        SmartGeyserBoostSwitch(coordinator, entry),
        SmartGeyserReadOnlySwitch(coordinator, entry),
    ])


class SmartGeyserBoostSwitch(CoordinatorEntity[SmartGeyserCoordinator], SwitchEntity):
    """Toggle 60-minute manual boost on/off."""

    _attr_has_entity_name = True
    _attr_name = "Manual Boost"
    _attr_device_class = SwitchDeviceClass.SWITCH
    _attr_icon = "mdi:fire"

    def __init__(
        self,
        coordinator: SmartGeyserCoordinator,
        entry: ConfigEntry,
    ) -> None:
        super().__init__(coordinator)
        self._attr_unique_id = f"{entry.entry_id}_boost"
        self._attr_device_info = {
            "identifiers": {(DOMAIN, entry.entry_id)},
            "name": "Smart Geyser Controller",
        }

    @property
    def is_on(self) -> bool:
        if self.coordinator.data is None:
            return False
        boost_until = self.coordinator.data.boost_until
        return boost_until is not None and boost_until > utcnow()

    async def async_turn_on(self, **kwargs: object) -> None:
        await self.coordinator.client.post_boost(DEFAULT_BOOST_MINUTES)
        await self.coordinator.async_request_refresh()

    async def async_turn_off(self, **kwargs: object) -> None:
        await self.coordinator.client.delete_boost()
        await self.coordinator.async_request_refresh()


class SmartGeyserReadOnlySwitch(CoordinatorEntity[SmartGeyserCoordinator], SwitchEntity):
    """Suspend element control — engine still observes and reports, but sends no commands."""

    _attr_has_entity_name = True
    _attr_name = "Read-Only Mode"
    _attr_device_class = SwitchDeviceClass.SWITCH
    _attr_icon = "mdi:eye-outline"

    def __init__(
        self,
        coordinator: SmartGeyserCoordinator,
        entry: ConfigEntry,
    ) -> None:
        super().__init__(coordinator)
        self._attr_unique_id = f"{entry.entry_id}_read_only"
        self._attr_device_info = {
            "identifiers": {(DOMAIN, entry.entry_id)},
            "name": "Smart Geyser Controller",
        }

    @property
    def is_on(self) -> bool:
        if self.coordinator.data is None:
            return False
        return self.coordinator.data.read_only_mode

    async def async_turn_on(self, **kwargs: object) -> None:
        await self.coordinator.client.enable_read_only()
        await self.coordinator.async_request_refresh()

    async def async_turn_off(self, **kwargs: object) -> None:
        await self.coordinator.client.disable_read_only()
        await self.coordinator.async_request_refresh()
