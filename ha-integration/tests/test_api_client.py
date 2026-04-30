"""Tests for SmartGeyserClient."""
from __future__ import annotations

import pytest
from aioresponses import aioresponses
import aiohttp

from custom_components.smart_geyser.api_client import (
    CannotConnect,
    GeyserStatus,
    InvalidResponse,
    SmartGeyserClient,
)


STATUS_PAYLOAD = {
    "system_type": "solar_pumped",
    "provider": "Geyserwala Connect",
    "setpoint_c": 60.0,
    "tank_temp_c": 58.5,
    "collector_temp_c": 72.1,
    "pump_active": True,
    "heating_active": False,
    "smart_stop_active": False,
    "preheat_active": False,
    "boost_until": None,
    "next_predicted_use": None,
    "preheat_starts_at": None,
    "events_today": 2,
}


@pytest.fixture
async def client():
    async with aiohttp.ClientSession() as session:
        yield SmartGeyserClient(session, "localhost", 8080)


@pytest.mark.asyncio
async def test_get_status_happy_path(client):
    with aioresponses() as m:
        m.get("http://localhost:8080/api/status", payload=STATUS_PAYLOAD)
        status = await client.get_status()

    assert isinstance(status, GeyserStatus)
    assert status.tank_temp_c == 58.5
    assert status.collector_temp_c == 72.1
    assert status.pump_active is True
    assert status.heating_active is False
    assert status.events_today == 2
    assert status.pv is None


@pytest.mark.asyncio
async def test_get_status_connection_error(client):
    with aioresponses() as m:
        m.get(
            "http://localhost:8080/api/status",
            exception=aiohttp.ClientConnectionError("refused"),
        )
        with pytest.raises(CannotConnect):
            await client.get_status()


@pytest.mark.asyncio
async def test_get_status_500_raises_invalid_response(client):
    with aioresponses() as m:
        m.get("http://localhost:8080/api/status", status=500, payload={"error": "oops"})
        with pytest.raises(InvalidResponse):
            await client.get_status()


@pytest.mark.asyncio
async def test_post_boost(client):
    with aioresponses() as m:
        m.post(
            "http://localhost:8080/api/boost",
            payload={"ok": True, "boost_until": "2026-04-30T10:00:00Z"},
        )
        await client.post_boost(60)  # should not raise


@pytest.mark.asyncio
async def test_delete_boost(client):
    with aioresponses() as m:
        m.delete("http://localhost:8080/api/boost", payload={"ok": True})
        await client.delete_boost()  # should not raise


@pytest.mark.asyncio
async def test_post_setpoint(client):
    with aioresponses() as m:
        m.post("http://localhost:8080/api/setpoint", payload={"ok": True})
        await client.post_setpoint(65.0)  # should not raise


@pytest.mark.asyncio
async def test_status_with_boost_until_parsed(client):
    payload = {**STATUS_PAYLOAD, "boost_until": "2026-04-30T10:00:00Z"}
    with aioresponses() as m:
        m.get("http://localhost:8080/api/status", payload=payload)
        status = await client.get_status()

    assert status.boost_until is not None
    assert status.boost_until.year == 2026
