"""DataUpdateCoordinator for smart-geyser-service."""
from __future__ import annotations

import asyncio
import logging
from datetime import timedelta

from homeassistant.core import HomeAssistant
from homeassistant.helpers.update_coordinator import DataUpdateCoordinator, UpdateFailed

from .api_client import CannotConnect, GeyserConfig, GeyserStatus, InvalidResponse, SmartGeyserClient
from .const import DOMAIN, SCAN_INTERVAL

_LOGGER = logging.getLogger(__name__)


class SmartGeyserCoordinator(DataUpdateCoordinator[GeyserStatus]):
    """Polls /api/status on a fixed interval; also subscribes to SSE for push updates."""

    def __init__(self, hass: HomeAssistant, client: SmartGeyserClient) -> None:
        super().__init__(
            hass,
            _LOGGER,
            name=DOMAIN,
            update_interval=timedelta(seconds=SCAN_INTERVAL),
        )
        self.client = client
        self.addon_config: GeyserConfig | None = None
        self._sse_task: asyncio.Task | None = None

    async def _async_update_data(self) -> GeyserStatus:
        try:
            return await self.client.get_status()
        except CannotConnect as exc:
            raise UpdateFailed(f"Cannot connect to smart-geyser-service: {exc}") from exc
        except InvalidResponse as exc:
            raise UpdateFailed(f"Unexpected response from service: {exc}") from exc

    def start_sse_listener(self) -> None:
        """Start background SSE task; push updates instantly as they arrive."""
        self._sse_task = self.hass.loop.create_task(
            self._listen_sse(), name="smart_geyser_sse"
        )

    def stop_sse_listener(self) -> None:
        """Cancel the background SSE task."""
        if self._sse_task is not None:
            self._sse_task.cancel()
            self._sse_task = None

    async def _listen_sse(self) -> None:
        while True:
            try:
                async for status in self.client.stream_status():
                    self.async_set_updated_data(status)
            except asyncio.CancelledError:
                return
            except Exception as exc:  # noqa: BLE001
                _LOGGER.warning("SSE stream lost (%s) — reconnecting in 5 s", exc)
                await asyncio.sleep(5)
