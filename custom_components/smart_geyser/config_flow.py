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
    """Multi-step options flow — configure provider hardware or engine settings."""

    def __init__(self, config_entry: ConfigEntry) -> None:
        self._entry = config_entry
        self._provider_type: str = "geyserwala"
        self._current_provider: dict = {}
        self._current_engine: dict = {}

    def _client(self) -> SmartGeyserClient:
        session = async_get_clientsession(self.hass)
        return SmartGeyserClient(
            session,
            self._entry.data[CONF_HOST],
            self._entry.data.get(CONF_PORT, DEFAULT_PORT),
        )

    # ------------------------------------------------------------------
    # Step 1: menu — what would you like to configure?
    # ------------------------------------------------------------------

    async def async_step_init(
        self, user_input: dict | None = None
    ) -> ConfigFlowResult:
        """Choose whether to configure provider settings or engine settings."""
        if user_input is not None:
            section = user_input["section"]
            if section == "engine":
                return await self.async_step_engine_config()
            return await self.async_step_provider_type()

        return self.async_show_form(
            step_id="init",
            data_schema=vol.Schema(
                {
                    vol.Required("section", default="provider"): vol.In(
                        ["provider", "engine"]
                    ),
                }
            ),
        )

    # ------------------------------------------------------------------
    # Step 2a: choose provider type
    # ------------------------------------------------------------------

    async def async_step_provider_type(
        self, user_input: dict | None = None
    ) -> ConfigFlowResult:
        """Choose between REST and MQTT provider."""
        if not self._current_provider:
            try:
                self._current_provider = await self._client().get_provider_config()
            except Exception:  # noqa: BLE001
                self._current_provider = {}

        if user_input is not None:
            self._provider_type = user_input["provider_type"]
            if self._provider_type == "geyserwala_mqtt":
                return await self.async_step_mqtt_config()
            return await self.async_step_rest_config()

        current_type = self._current_provider.get("type", "geyserwala")
        return self.async_show_form(
            step_id="provider_type",
            data_schema=vol.Schema(
                {
                    vol.Required("provider_type", default=current_type): vol.In(
                        ["geyserwala", "geyserwala_mqtt"]
                    ),
                }
            ),
        )

    # ------------------------------------------------------------------
    # Step 2b: REST provider settings
    # ------------------------------------------------------------------

    async def async_step_rest_config(
        self, user_input: dict | None = None
    ) -> ConfigFlowResult:
        """Geyserwala REST connection settings."""
        errors: dict[str, str] = {}
        c = self._current_provider

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

    # ------------------------------------------------------------------
    # Step 2b (alt): MQTT provider settings
    # ------------------------------------------------------------------

    async def async_step_mqtt_config(
        self, user_input: dict | None = None
    ) -> ConfigFlowResult:
        """Geyserwala MQTT connection settings."""
        errors: dict[str, str] = {}
        c = self._current_provider

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

    # ------------------------------------------------------------------
    # Step 2c: engine / scheduler settings
    # ------------------------------------------------------------------

    async def async_step_engine_config(
        self, user_input: dict | None = None
    ) -> ConfigFlowResult:
        """Engine and scheduler settings."""
        errors: dict[str, str] = {}

        if not self._current_engine:
            try:
                self._current_engine = await self._client().get_engine_config()
            except Exception:  # noqa: BLE001
                self._current_engine = {}

        e = self._current_engine

        if user_input is not None:
            try:
                await self._client().set_engine_config(
                    {
                        "setpoint_c": float(user_input["setpoint_c"]),
                        "hysteresis_c": float(user_input["hysteresis_c"]),
                        "preheat_threshold": float(user_input["preheat_threshold"]),
                        "late_use_threshold": float(user_input["late_use_threshold"]),
                        "cutoff_buffer_min": int(user_input["cutoff_buffer_min"]),
                        "safety_margin_min": int(user_input["safety_margin_min"]),
                        "decay_factor": float(user_input["decay_factor"]),
                        "legionella_interval_days": int(user_input["legionella_interval_days"]),
                        "tick_interval_secs": int(user_input["tick_interval_secs"]),
                    }
                )
                return self.async_create_entry(title="", data={})
            except CannotConnect:
                errors["base"] = "cannot_connect"
            except Exception:  # noqa: BLE001
                errors["base"] = "save_failed"

        return self.async_show_form(
            step_id="engine_config",
            data_schema=vol.Schema(
                {
                    vol.Required(
                        "setpoint_c", default=e.get("setpoint_c", 60.0)
                    ): vol.All(vol.Coerce(float), vol.Range(min=40, max=75)),
                    vol.Required(
                        "hysteresis_c", default=e.get("hysteresis_c", 4.0)
                    ): vol.All(vol.Coerce(float), vol.Range(min=0, max=20)),
                    vol.Required(
                        "preheat_threshold", default=e.get("preheat_threshold", 0.4)
                    ): vol.All(vol.Coerce(float), vol.Range(min=0, max=1)),
                    vol.Required(
                        "late_use_threshold", default=e.get("late_use_threshold", 0.15)
                    ): vol.All(vol.Coerce(float), vol.Range(min=0, max=1)),
                    vol.Required(
                        "cutoff_buffer_min", default=e.get("cutoff_buffer_min", 30)
                    ): vol.All(vol.Coerce(int), vol.Range(min=0, max=120)),
                    vol.Required(
                        "safety_margin_min", default=e.get("safety_margin_min", 20)
                    ): vol.All(vol.Coerce(int), vol.Range(min=0, max=60)),
                    vol.Required(
                        "decay_factor", default=e.get("decay_factor", 0.995)
                    ): vol.All(vol.Coerce(float), vol.Range(min=0.9, max=1.0)),
                    vol.Required(
                        "legionella_interval_days",
                        default=e.get("legionella_interval_days", 7),
                    ): vol.All(vol.Coerce(int), vol.Range(min=1, max=30)),
                    vol.Required(
                        "tick_interval_secs", default=e.get("tick_interval_secs", 60)
                    ): vol.All(vol.Coerce(int), vol.Range(min=1, max=3600)),
                }
            ),
            errors=errors,
        )
