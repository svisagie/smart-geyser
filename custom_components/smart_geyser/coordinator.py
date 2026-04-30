"""DataUpdateCoordinator for smart-geyser-service."""
from __future__ import annotations

import logging
from datetime import timedelta

from homeassistant.core import HomeAssistant
from homeassistant.helpers.update_coordinator import DataUpdateCoordinator, UpdateFailed

from .api_client import CannotConnect, GeyserStatus, InvalidResponse, SmartGeyserClient
from .const import DOMAIN, SCAN_INTERVAL

_LOGGER = logging.getLogger(__name__)


class SmartGeyserCoordinator(DataUpdateCoordinator[GeyserStatus]):
    """Polls /api/status on a fixed interval."""

    def __init__(self, hass: HomeAssistant, client: SmartGeyserClient) -> None:
        super().__init__(
            hass,
            _LOGGER,
            name=DOMAIN,
            update_interval=timedelta(seconds=SCAN_INTERVAL),
        )
        self.client = client

    async def _async_update_data(self) -> GeyserStatus:
        try:
            return await self.client.get_status()
        except CannotConnect as exc:
            raise UpdateFailed(f"Cannot connect to smart-geyser-service: {exc}") from exc
        except InvalidResponse as exc:
            raise UpdateFailed(f"Unexpected response from service: {exc}") from exc
