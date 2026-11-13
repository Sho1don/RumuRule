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
const tunInfo = document.querySelector("#tun-info");
const pingBlock = document.querySelector("#ping-block");
const pingToggle = document.querySelector("#ping-toggle");
const pingSummary = document.querySelector("#ping-summary");
const splitBlock = document.querySelector("#split-block");
const splitToggle = document.querySelector("#split-toggle");
const splitMode = document.querySelector("#split-mode");
const splitEntries = document.querySelector("#split-entries");
const splitState = document.querySelector("#split-state");
const coreVersion = document.querySelector("#core-version");

let current = null;
/// Whether the one-time privileged setup covers this user. Without it the
/// system asks for a password at every tunnel start.
let setupInstalled = false;
/// The setup offer is shown once per run, when TUN is switched on.
let setupOffered = false;

const isProxy = (kind) => kind === "proxy";

const PING_LABELS = { proxy: "Via proxy", tcp: "TCP", icmp: "ICMP" };
const PING_COLLAPSED_KEY = "rumurule.pingCollapsed";
const SPLIT_COLLAPSED_KEY = "rumurule.splitCollapsed";

/** Collapsed the way server groups are: remembered across restarts. */
function setPingCollapsed(collapsed) {
  pingBlock.dataset.collapsed = String(collapsed);
  pingToggle.setAttribute("aria-expanded", String(!collapsed));
  localStorage.setItem(PING_COLLAPSED_KEY, String(collapsed));
}

function pingSummaryText(kind, mode) {
  const label = PING_LABELS[kind] ?? "TCP";
  return isProxy(kind) ? `${label} · ${mode}` : label;
}

function setSplitCollapsed(collapsed) {
  splitBlock.dataset.collapsed = String(collapsed);
  splitToggle.setAttribute("aria-expanded", String(!collapsed));
  localStorage.setItem(SPLIT_COLLAPSED_KEY, String(collapsed));
}

function splitSummaryText(mode, count) {
  return mode === "off" ? "off" : `${mode} · ${count}`;
}

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
  pingSummary.textContent = pingSummaryText(settings.pingKind, settings.pingMode);

  const tun = settings.tunnelMode === "tun";
  tunToggle.checked = tun;
  showIpToggle.checked = settings.showAddress === true;

  const split = settings.split ?? { mode: "off", entries: [] };
  splitMode.value = split.mode ?? "off";
  const entries = (split.entries ?? []).join("\n");
  // Never rewrite the textarea under the cursor.
  if (document.activeElement !== splitEntries) splitEntries.value = entries;
  splitState.textContent = splitSummaryText(split.mode, split.entries?.length ?? 0);
}

async function saveSplit() {
  const mode = splitMode.value;
  const entries = splitEntries.value
    .split("\n")
    .map((entry) => entry.trim())
    .filter(Boolean);
  try {
    const settings = await invoke("settings_set_split", { mode, entries });
    current = settings;
    splitState.textContent = splitSummaryText(mode, entries.length);
    appendLog(`split tunneling: ${mode}, ${entries.length} entries`);
  } catch (error) {
    appendLog(`split error: ${error}`);
  }
}

async function refreshPrereqs() {
  try {
    const setup = await invoke("setup_state").catch(() => null);
    setupInstalled = setup?.installed === true && setup?.userMatches === true;
    const prereqs = await invoke("tun_prereqs");
    if (!prereqs.helper) {
      tunStatus.textContent = "helper missing";
    } else if (!prereqs.polkit) {
      tunStatus.textContent = "polkit missing";
    } else {
      tunStatus.textContent = "ready";
    }
    // Paths, the IP stack and errors are diagnostics: one hover away, not a
    // permanent line under the switch.
    const parts = [];
    if (prereqs.helper) parts.push(`helper ${prereqs.helper}`);
    if (prereqs.xray) parts.push(`xray ${prereqs.xray}`);
    parts.push(prereqs.ipv6 ? "IPv4 + IPv6" : "IPv4 only");
    if (prereqs.error) parts.push(prereqs.error);
    tunInfo.title = parts.join(" · ");
    tunInfo.hidden = false;
    coreVersion.textContent = prereqs.xrayVersion
      ? `Xray ${prereqs.xrayVersion}`
      : "Xray not found";
  } catch (error) {
    tunStatus.textContent = "unavailable";
    tunInfo.title = String(error);
    tunInfo.hidden = false;
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
      if (mode === "tun") {
        refreshPrereqs();
        // Turning TUN on is where the password starts to matter. Offer the
        // one-time setup here instead of leaving a button in the panel.
        if (!setupInstalled && !setupOffered) {
          setupOffered = true;
          openWelcome();
        }
      }
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

  pingToggle.addEventListener("click", () => {
    setPingCollapsed(pingBlock.dataset.collapsed !== "true");
  });
  setPingCollapsed(localStorage.getItem(PING_COLLAPSED_KEY) === "true");

  splitToggle.addEventListener("click", () => {
    setSplitCollapsed(splitBlock.dataset.collapsed !== "true");
  });
  setSplitCollapsed(localStorage.getItem(SPLIT_COLLAPSED_KEY) === "true");

  splitMode.addEventListener("change", saveSplit);
  splitEntries.addEventListener("change", saveSplit);

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
