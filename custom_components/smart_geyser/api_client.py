"""Async HTTP client for the smart-geyser-service REST API."""
from __future__ import annotations

import json
from dataclasses import dataclass, field
from datetime import datetime
from typing import Any, AsyncGenerator

import aiohttp


class CannotConnect(Exception):
    """Raised when the service is unreachable."""


class InvalidResponse(Exception):
    """Raised when the service returns an unexpected response."""


@dataclass
class GeyserConfig:
    """Parsed /api/config response — static for the lifetime of the add-on."""

    setpoint_c: float
    hysteresis_c: float
    preheat_threshold: float
    late_use_threshold: float
    cutoff_buffer_min: int
    safety_margin_min: int
    legionella_interval_days: int
    decay_factor: float
    tick_interval_secs: int

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> "GeyserConfig":
        return cls(
            setpoint_c=data["setpoint_c"],
            hysteresis_c=data["hysteresis_c"],
            preheat_threshold=data["preheat_threshold"],
            late_use_threshold=data["late_use_threshold"],
            cutoff_buffer_min=data["cutoff_buffer_min"],
            safety_margin_min=data["safety_margin_min"],
            legionella_interval_days=data["legionella_interval_days"],
            decay_factor=data["decay_factor"],
            tick_interval_secs=data["tick_interval_secs"],
        )


@dataclass
class GeyserStatus:
    """Parsed /api/status response."""

    system_type: str
    provider: str
    setpoint_c: float
    tank_temp_c: float | None
    collector_temp_c: float | None
    pump_active: bool | None
    heating_active: bool | None
    smart_stop_active: bool
    preheat_active: bool
    read_only_mode: bool
    boost_until: datetime | None
    next_predicted_use: datetime | None
    preheat_starts_at: datetime | None
    events_today: int
    # PV fields (None in v1)
    pv: dict[str, Any] | None = field(default=None)
    # Energy fields (None in v1)
    energy_today: dict[str, Any] | None = field(default=None)
    energy_30d: dict[str, Any] | None = field(default=None)

    @staticmethod
    def _parse_dt(value: str | None) -> datetime | None:
        if value is None:
            return None
        return datetime.fromisoformat(value.replace("Z", "+00:00"))

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> "GeyserStatus":
        return cls(
            system_type=data["system_type"],
            provider=data["provider"],
            setpoint_c=data["setpoint_c"],
            tank_temp_c=data.get("tank_temp_c"),
            collector_temp_c=data.get("collector_temp_c"),
            pump_active=data.get("pump_active"),
            heating_active=data.get("heating_active"),
            smart_stop_active=data.get("smart_stop_active", False),
            preheat_active=data.get("preheat_active", False),
            read_only_mode=data.get("read_only_mode", False),
            boost_until=cls._parse_dt(data.get("boost_until")),
            next_predicted_use=cls._parse_dt(data.get("next_predicted_use")),
            preheat_starts_at=cls._parse_dt(data.get("preheat_starts_at")),
            events_today=data.get("events_today", 0),
            pv=data.get("pv"),
            energy_today=data.get("energy_today"),
            energy_30d=data.get("energy_30d"),
        )


class SmartGeyserClient:
    """Thin async wrapper around the Rust service REST API."""

    def __init__(
        self,
        session: aiohttp.ClientSession,
        host: str,
        port: int = 8080,
        timeout: int = 10,
    ) -> None:
        self._session = session
        self._base = f"http://{host}:{port}"
        self._timeout = aiohttp.ClientTimeout(total=timeout)

    async def _get(self, path: str) -> Any:
        try:
            async with self._session.get(
                f"{self._base}{path}", timeout=self._timeout
            ) as resp:
                if resp.status >= 500:
                    raise InvalidResponse(f"HTTP {resp.status} from {path}")
                return await resp.json()
        except aiohttp.ClientConnectionError as exc:
            raise CannotConnect(f"Cannot connect to {self._base}: {exc}") from exc
        except aiohttp.ClientError as exc:
            raise CannotConnect(str(exc)) from exc

    async def _post(self, path: str, payload: dict[str, Any]) -> dict[str, Any]:
        try:
            async with self._session.post(
                f"{self._base}{path}",
                json=payload,
                timeout=self._timeout,
            ) as resp:
                return await resp.json()
        except aiohttp.ClientConnectionError as exc:
            raise CannotConnect(f"Cannot connect to {self._base}: {exc}") from exc
        except aiohttp.ClientError as exc:
            raise CannotConnect(str(exc)) from exc

    async def _delete(self, path: str) -> dict[str, Any]:
        try:
            async with self._session.delete(
                f"{self._base}{path}", timeout=self._timeout
            ) as resp:
                return await resp.json()
        except aiohttp.ClientConnectionError as exc:
            raise CannotConnect(f"Cannot connect to {self._base}: {exc}") from exc
        except aiohttp.ClientError as exc:
            raise CannotConnect(str(exc)) from exc

    async def get_config(self) -> GeyserConfig:
        """Return the add-on's current configuration."""
        try:
            data = await self._get("/api/config")
            return GeyserConfig.from_dict(data)
        except (KeyError, TypeError, ValueError) as exc:
            raise InvalidResponse(f"Unexpected /api/config shape: {exc}") from exc

    async def enable_read_only(self) -> None:
        """Put the service into read-only (observe-only) mode."""
        await self._post("/api/read-only", {})

    async def disable_read_only(self) -> None:
        """Resume normal element control."""
        await self._delete("/api/read-only")

    async def get_status(self) -> GeyserStatus:
        """Return the current service status."""
        try:
            data = await self._get("/api/status")
            return GeyserStatus.from_dict(data)
        except (KeyError, TypeError, ValueError) as exc:
            raise InvalidResponse(f"Unexpected /api/status shape: {exc}") from exc

    async def post_boost(self, duration_minutes: int) -> None:
        """Trigger a manual boost for `duration_minutes` (1–480)."""
        await self._post("/api/boost", {"duration_minutes": duration_minutes})

    async def delete_boost(self) -> None:
        """Cancel any active boost."""
        await self._delete("/api/boost")

    async def post_setpoint(self, temp_c: float) -> None:
        """Update the heating setpoint (40–75 °C)."""
        await self._post("/api/setpoint", {"temp_c": temp_c})

    async def stream_status(self) -> AsyncGenerator[GeyserStatus, None]:
        """Yield GeyserStatus objects from the SSE /api/events endpoint."""
        sse_timeout = aiohttp.ClientTimeout(total=None, connect=10, sock_read=None)
        try:
            async with self._session.get(
                f"{self._base}/api/events",
                timeout=sse_timeout,
            ) as resp:
                async for raw_line in resp.content:
                    line = raw_line.decode("utf-8").strip()
                    if line.startswith("data:"):
                        payload = line[5:].strip()
                        if payload:
                            try:
                                yield GeyserStatus.from_dict(json.loads(payload))
                            except (json.JSONDecodeError, KeyError, TypeError):
                                pass
        except aiohttp.ClientConnectionError as exc:
            raise CannotConnect(f"SSE connection lost: {exc}") from exc
