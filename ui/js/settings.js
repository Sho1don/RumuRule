// Device binding and latency probe settings.

import { invoke } from "./api.js";
import { appendLog } from "./log.js";
import { openWelcome } from "./welcome.js";

const toggle = document.querySelector("#hwid-toggle");
const value = document.querySelector("#hwid-value");
const reset = document.querySelector("#hwid-reset");
const proxyToggle = document.querySelector("#proxy-toggle");
const pingKinds = [...document.querySelectorAll('input[name="ping-kind"]')];
const pingMode = document.querySelector("#ping-mode");
const pingUrl = document.querySelector("#ping-url");
const pingProxy = document.querySelector("#ping-proxy");
const tunToggle = document.querySelector("#tun-toggle");
const showIpToggle = document.querySelector("#show-ip-toggle");
const tunStatus = document.querySelector("#tun-status");
const tunExtra = document.querySelector("#tun-extra");
const tunDetail = document.querySelector("#tun-detail");
const tunAuthorize = document.querySelector("#tun-authorize");
const coreVersion = document.querySelector("#core-version");

let current = null;

const isProxy = (kind) => kind === "proxy";

function render(settings) {
  current = settings;
  toggle.checked = settings.hwidEnabled;
  value.textContent = settings.hwid;
  value.title = settings.hwid;
  proxyToggle.checked = settings.systemProxy;

  for (const radio of pingKinds) {
    radio.checked = radio.value === settings.pingKind;
  }
  pingMode.value = settings.pingMode;
  pingUrl.value = settings.pingUrl;
  pingProxy.hidden = !isProxy(settings.pingKind);

  const tun = settings.tunnelMode === "tun";
  tunToggle.checked = tun;
  tunExtra.hidden = !tun;
  showIpToggle.checked = settings.showAddress === true;
}

async function refreshPrereqs() {
  try {
    const prereqs = await invoke("tun_prereqs");
    if (!prereqs.helper) {
      tunStatus.textContent = "helper missing";
    } else if (!prereqs.polkit) {
      tunStatus.textContent = "polkit missing";
    } else {
      tunStatus.textContent = "ready";
    }
    const parts = [];
    if (prereqs.helper) parts.push(`helper ${prereqs.helper}`);
    if (prereqs.xray) parts.push(`xray ${prereqs.xray}`);
    parts.push(prereqs.ipv6 ? "IPv4 + IPv6" : "IPv4 only");
    if (prereqs.error) parts.push(prereqs.error);
    tunDetail.textContent = parts.join(" · ");
    coreVersion.textContent = prereqs.xrayVersion
      ? `Xray ${prereqs.xrayVersion}`
      : "Xray not found";
    tunAuthorize.disabled = !prereqs.helper || !prereqs.polkit;
  } catch (error) {
    tunStatus.textContent = "unavailable";
    tunDetail.textContent = String(error);
  }
}

async function savePing() {
  const kind = pingKinds.find((radio) => radio.checked)?.value ?? "tcp";
  try {
    const settings = await invoke("settings_set_ping", {
      kind,
      mode: pingMode.value,
      url: pingUrl.value,
    });
    render(settings);
    window.dispatchEvent(
      new CustomEvent("rumurule:ping-settings", { detail: settings }),
    );
    appendLog(`ping mode: ${kind}`);
  } catch (error) {
    render(current);
    appendLog(`settings error: ${error}`);
  }
}

/** Load settings and wire the controls. */
export async function initSettings() {
  try {
    render(await invoke("settings_get"));
  } catch (error) {
    appendLog(`settings error: ${error}`);
  }

  toggle.addEventListener("change", async () => {
    try {
      render(await invoke("settings_set_hwid", { enabled: toggle.checked }));
      appendLog(`hwid ${toggle.checked ? "enabled" : "disabled"}`);
    } catch (error) {
      toggle.checked = current?.hwidEnabled ?? true;
      appendLog(`settings error: ${error}`);
    }
  });

  proxyToggle.addEventListener("change", async () => {
    try {
      render(await invoke("settings_set_system_proxy", { enabled: proxyToggle.checked }));
      appendLog(
        `system proxy ${proxyToggle.checked ? "enabled" : "disabled"}`,
      );
    } catch (error) {
      proxyToggle.checked = current?.systemProxy ?? true;
      appendLog(`settings error: ${error}`);
    }
  });

  tunToggle.addEventListener("change", async () => {
    const mode = tunToggle.checked ? "tun" : "proxy";
    try {
      render(await invoke("settings_set_tunnel_mode", { mode }));
      appendLog(`tunnel mode: ${mode}`);
      if (mode === "tun") refreshPrereqs();
    } catch (error) {
      // The mode cannot change mid-session; put the switch back.
      render(current);
      appendLog(`settings error: ${error}`);
    }
  });

  showIpToggle.addEventListener("change", async () => {
    try {
      const settings = await invoke("settings_set_show_address", {
        enabled: showIpToggle.checked,
      });
      render(settings);
      // The server list and the status tab repaint with the new privacy.
      window.dispatchEvent(
        new CustomEvent("rumurule:show-address", { detail: settings.showAddress }),
      );
      appendLog(`show addresses ${showIpToggle.checked ? "on" : "off"}`);
    } catch (error) {
      showIpToggle.checked = current?.showAddress === true;
      appendLog(`settings error: ${error}`);
    }
  });

  tunAuthorize.addEventListener("click", async () => {
    tunAuthorize.disabled = true;
    tunAuthorize.textContent = "Waiting for approval…";
    try {
      const report = await invoke("tun_authorize");
      appendLog(`tunnel ready: ${report}`);
    } catch (error) {
      appendLog(`tunnel error: ${error}`);
    } finally {
      tunAuthorize.textContent = "Allow tunnel";
      refreshPrereqs();
    }
  });

  document
    .querySelector("#tun-setup")
    .addEventListener("click", () => openWelcome());

  refreshPrereqs();

  for (const radio of pingKinds) {
    radio.addEventListener("change", () => {
      if (!radio.checked) return;
      pingProxy.hidden = !isProxy(radio.value);
      savePing();
    });
  }
  pingMode.addEventListener("change", savePing);
  pingUrl.addEventListener("change", savePing);

  reset.addEventListener("click", async () => {
    try {
      render(await invoke("settings_reset_hwid"));
      appendLog("hwid regenerated");
    } catch (error) {
      appendLog(`settings error: ${error}`);
    }
  });
}
