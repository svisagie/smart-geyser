"""Config flow for Smart Geyser Controller."""
from __future__ import annotations

import voluptuous as vol

from homeassistant.config_entries import ConfigEntry, ConfigFlow, ConfigFlowResult, OptionsFlow
from homeassistant.helpers.aiohttp_client import async_get_clientsession

from .api_client import CannotConnect, SmartGeyserClient
from .const import CONF_HOST, CONF_PORT, DEFAULT_HOST, DEFAULT_PORT, DOMAIN


class SmartGeyserConfigFlow(ConfigFlow, domain=DOMAIN):
    """Handle a config flow for Smart Geyser Controller."""

    VERSION = 1

    @staticmethod
    def async_get_options_flow(config_entry: ConfigEntry) -> SmartGeyserOptionsFlow:
        return SmartGeyserOptionsFlow(config_entry)

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
            except Exception:  # noqa: BLE001
                errors["base"] = "unknown"
            else:
                await self.async_set_unique_id(f"{host}:{port}")
                self._abort_if_unique_id_configured()
                return self.async_create_entry(
                    title=f"Smart Geyser ({status.provider})",
                    data={CONF_HOST: host, CONF_PORT: port},
                )

        return self.async_show_form(
            step_id="user",
            data_schema=vol.Schema(
                {
                    vol.Required(CONF_HOST, default=DEFAULT_HOST): str,
                    vol.Required(CONF_PORT, default=DEFAULT_PORT): vol.Coerce(int),
                }
            ),
            errors=errors,
        )


class SmartGeyserOptionsFlow(OptionsFlow):
    """Allow the user to reconfigure the geyser provider through the HA UI."""

    def __init__(self, config_entry: ConfigEntry) -> None:
        self._entry = config_entry
        self._provider_type: str = "geyserwala"
        self._current: dict = {}

    def _client(self) -> SmartGeyserClient:
        session = async_get_clientsession(self.hass)
        return SmartGeyserClient(
            session,
            self._entry.data[CONF_HOST],
            self._entry.data.get(CONF_PORT, DEFAULT_PORT),
        )

    async def async_step_init(
        self, user_input: dict | None = None
    ) -> ConfigFlowResult:
        """Step 1 — choose provider type."""
        if not self._current:
            try:
                self._current = await self._client().get_provider_config()
            except Exception:  # noqa: BLE001
                self._current = {}

        if user_input is not None:
            self._provider_type = user_input["provider_type"]
            if self._provider_type == "geyserwala_mqtt":
                return await self.async_step_mqtt_config()
            return await self.async_step_rest_config()

        current_type = self._current.get("type", "geyserwala")
        return self.async_show_form(
            step_id="init",
            data_schema=vol.Schema(
                {
                    vol.Required("provider_type", default=current_type): vol.In(
                        ["geyserwala", "geyserwala_mqtt"]
                    ),
                }
            ),
        )

    async def async_step_rest_config(
        self, user_input: dict | None = None
    ) -> ConfigFlowResult:
        """Step 2a — Geyserwala REST connection settings."""
        errors: dict[str, str] = {}
        c = self._current

        if user_input is not None:
            try:
                await self._client().set_provider_config(
                    {
                        "type": "geyserwala",
                        "base_url": user_input["base_url"],
                        "token": user_input.get("token") or None,
                        "element_kw": float(user_input["element_kw"]),
                        "tank_volume_l": float(user_input["tank_volume_l"]),
                        "timeout_secs": 10,
                    }
                )
                return self.async_create_entry(title="", data={})
            except CannotConnect:
                errors["base"] = "cannot_connect"
            except Exception:  # noqa: BLE001
                errors["base"] = "save_failed"

        return self.async_show_form(
            step_id="rest_config",
            data_schema=vol.Schema(
                {
                    vol.Required("base_url", default=c.get("base_url", "http://")): str,
                    vol.Optional("token", default=c.get("token") or ""): str,
                    vol.Required(
                        "element_kw", default=c.get("element_kw", 3.0)
                    ): vol.All(vol.Coerce(float), vol.Range(min=0.5, max=10.0)),
                    vol.Required(
                        "tank_volume_l", default=c.get("tank_volume_l", 150.0)
                    ): vol.All(vol.Coerce(float), vol.Range(min=50, max=500)),
                }
            ),
            errors=errors,
        )

    async def async_step_mqtt_config(
        self, user_input: dict | None = None
    ) -> ConfigFlowResult:
        """Step 2b — Geyserwala MQTT connection settings."""
        errors: dict[str, str] = {}
        c = self._current

        if user_input is not None:
            try:
                await self._client().set_provider_config(
                    {
                        "type": "geyserwala_mqtt",
                        "broker_host": user_input["broker_host"],
                        "broker_port": int(user_input["broker_port"]),
                        "device_id": user_input["device_id"],
                        "topic_prefix": user_input.get("topic_prefix") or "geyserwala",
                        "username": user_input.get("username") or None,
                        "password": user_input.get("password") or None,
                        "element_kw": float(user_input["element_kw"]),
                        "tank_volume_l": float(user_input["tank_volume_l"]),
                    }
                )
                return self.async_create_entry(title="", data={})
            except CannotConnect:
                errors["base"] = "cannot_connect"
            except Exception:  # noqa: BLE001
                errors["base"] = "save_failed"

        return self.async_show_form(
            step_id="mqtt_config",
            data_schema=vol.Schema(
                {
                    vol.Required("broker_host", default=c.get("broker_host", "")): str,
                    vol.Required(
                        "broker_port", default=c.get("broker_port", 1883)
                    ): vol.All(vol.Coerce(int), vol.Range(min=1, max=65535)),
                    vol.Required("device_id", default=c.get("device_id", "")): str,
                    vol.Required(
                        "topic_prefix", default=c.get("topic_prefix", "geyserwala")
                    ): str,
                    vol.Optional("username", default=c.get("username") or ""): str,
                    vol.Optional("password", default=c.get("password") or ""): str,
                    vol.Required(
                        "element_kw", default=c.get("element_kw", 3.0)
                    ): vol.All(vol.Coerce(float), vol.Range(min=0.5, max=10.0)),
                    vol.Required(
                        "tank_volume_l", default=c.get("tank_volume_l", 150.0)
                    ): vol.All(vol.Coerce(float), vol.Range(min=50, max=500)),
                }
            ),
            errors=errors,
        )
