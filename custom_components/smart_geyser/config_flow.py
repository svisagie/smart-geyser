"""Config flow for Smart Geyser Controller."""
from __future__ import annotations

import voluptuous as vol
import aiohttp

from homeassistant.config_entries import ConfigFlow, ConfigFlowResult
from homeassistant.helpers.aiohttp_client import async_get_clientsession

from .api_client import CannotConnect, InvalidResponse, SmartGeyserClient
from .const import CONF_HOST, CONF_PORT, DEFAULT_HOST, DEFAULT_PORT, DOMAIN


class SmartGeyserConfigFlow(ConfigFlow, domain=DOMAIN):
    """Handle a config flow for Smart Geyser Controller."""

    VERSION = 1

    async def async_step_user(
        self, user_input: dict | None = None
    ) -> ConfigFlowResult:
        errors: dict[str, str] = {}

        if user_input is not None:
            host = user_input[CONF_HOST].strip()
            port = user_input[CONF_PORT]
            session = async_get_clientsession(self.hass)
            client = SmartGeyserClient(session, host, port)
            try:
                status = await client.get_status()
            except CannotConnect:
                errors["base"] = "cannot_connect"
            except InvalidResponse:
                errors["base"] = "invalid_response"
            except Exception:  # noqa: BLE001
                errors["base"] = "unknown"
            else:
                await self.async_set_unique_id(f"{host}:{port}")
                self._abort_if_unique_id_configured()
                return self.async_create_entry(
                    title=f"Smart Geyser ({status.provider})",
                    data={CONF_HOST: host, CONF_PORT: port},
                )

        schema = vol.Schema(
            {
                vol.Required(CONF_HOST, default=DEFAULT_HOST): str,
                vol.Required(CONF_PORT, default=DEFAULT_PORT): vol.Coerce(int),
            }
        )
        return self.async_show_form(
            step_id="user",
            data_schema=schema,
            errors=errors,
        )
