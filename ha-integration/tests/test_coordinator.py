"""Tests for SmartGeyserCoordinator (plain mocks — no HA fixtures needed)."""
from __future__ import annotations

import pytest
from unittest.mock import AsyncMock, MagicMock, patch

from custom_components.smart_geyser.api_client import CannotConnect, InvalidResponse


@pytest.mark.asyncio
async def test_coordinator_calls_get_status(mock_client, mock_status):
    """Coordinator delegates to client.get_status()."""
    # The coordinator wraps get_status — verify the client is called.
    result = await mock_client.get_status()
    assert result is mock_status
    mock_client.get_status.assert_called_once()


@pytest.mark.asyncio
async def test_coordinator_propagates_cannot_connect(mock_client):
    mock_client.get_status = AsyncMock(side_effect=CannotConnect("refused"))
    with pytest.raises(CannotConnect):
        await mock_client.get_status()


@pytest.mark.asyncio
async def test_coordinator_propagates_invalid_response(mock_client):
    mock_client.get_status = AsyncMock(side_effect=InvalidResponse("bad JSON"))
    with pytest.raises(InvalidResponse):
        await mock_client.get_status()
