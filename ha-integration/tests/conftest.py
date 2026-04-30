"""Shared fixtures — stubs HA modules so tests run without a real HA install."""
from __future__ import annotations

import sys
from unittest.mock import AsyncMock, MagicMock

# Stub every homeassistant.* import used by the custom component so the
# package loads without a real HA installation.
_HA_MODULES = [
    "homeassistant",
    "homeassistant.config_entries",
    "homeassistant.const",
    "homeassistant.core",
    "homeassistant.helpers",
    "homeassistant.helpers.aiohttp_client",
    "homeassistant.helpers.entity_platform",
    "homeassistant.helpers.entity",
    "homeassistant.helpers.update_coordinator",
    "homeassistant.components",
    "homeassistant.components.sensor",
    "homeassistant.components.binary_sensor",
    "homeassistant.components.number",
    "homeassistant.components.switch",
    "homeassistant.util",
    "homeassistant.util.dt",
    "voluptuous",
]
for _mod in _HA_MODULES:
    sys.modules.setdefault(_mod, MagicMock())

# NOW import the real custom component code (HA imports will hit the stubs).
import pytest  # noqa: E402
from custom_components.smart_geyser.api_client import GeyserStatus  # noqa: E402


@pytest.fixture
def mock_status() -> GeyserStatus:
    return GeyserStatus(
        system_type="solar_pumped",
        provider="Geyserwala Connect",
        setpoint_c=60.0,
        tank_temp_c=58.5,
        collector_temp_c=72.1,
        pump_active=True,
        heating_active=False,
        smart_stop_active=False,
        preheat_active=False,
        read_only_mode=False,
        boost_until=None,
        next_predicted_use=None,
        preheat_starts_at=None,
        events_today=2,
        pv=None,
    )


@pytest.fixture
def mock_client(mock_status):
    client = MagicMock()
    client.get_status = AsyncMock(return_value=mock_status)
    client.post_boost = AsyncMock()
    client.delete_boost = AsyncMock()
    client.post_setpoint = AsyncMock()
    return client
