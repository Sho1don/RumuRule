// Application wiring: state, commands and event handlers.

import { invoke, listen } from "./api.js";
import { appendLog } from "./log.js";
import { askName, openMenu } from "./menu.js";
import { initMotion } from "./motion.js";
import { pingLabel, pingTone, renderServers } from "./render.js";
import { initSettings } from "./settings.js";
import { initTabs } from "./tabs.js";
import { initTheme } from "./theme.js";
import { initWelcome } from "./welcome.js";

const stateBadge = document.querySelector("#state");
const noticeRoot = document.querySelector("#notice");
const homeState = document.querySelector("#home-state");
const homeServer = document.querySelector("#home-server");
const homeMeta = document.querySelector("#home-meta");
const homeError = document.querySelector("#home-error");
const homeErrorReason = document.querySelector("#home-error-reason");
const homeErrorDismiss = document.querySelector("#home-error-dismiss");
const homeView = document.querySelector("#view-home");
const homeUptime = document.querySelector("#home-uptime");
const homeWave = document.querySelector("#home-wave");
const homeConnect = document.querySelector("#home-connect");
const serversRoot = document.querySelector("#servers");
const subInput = document.querySelector("#sub-url");
const addSubButton = document.querySelector("#add-sub");

const COLLAPSED_KEY = "rumurule.collapsed";
const LAST_KEY = "rumurule.last";

let entries = [];
let subscriptions = [];
let running = false;
let connectedId = null;
let busy = false;
let pings = new Map();
let pingGeneration = 0;
let pingKind = "tcp";
let collapsed = new Set(loadCollapsed());
let statusInfo = null;
let lastId = localStorage.getItem(LAST_KEY);
let showAddress = false;
/** Latest connect stage reported by the backend, or null while none runs. */
let connectProgress = null;
/** Why the last connect attempt failed, shown until the next attempt. */
let connectError = null;

function loadCollapsed() {
  try {
    const raw = JSON.parse(localStorage.getItem(COLLAPSED_KEY) || "[]");
    return Array.isArray(raw) ? raw : [];
  } catch {
    return [];
  }
}

function saveCollapsed() {
  localStorage.setItem(COLLAPSED_KEY, JSON.stringify([...collapsed]));
}

let noticeTimer = null;

/** Shows a transient message above the dock. */
function showNotice(text, tone = "error") {
  if (!noticeRoot) return;
  noticeRoot.textContent = text;
  noticeRoot.dataset.tone = tone;
  noticeRoot.dataset.visible = "true";
  clearTimeout(noticeTimer);
  noticeTimer = setTimeout(() => {
    noticeRoot.dataset.visible = "false";
  }, 9000);
}

function setState(state, text) {
  stateBadge.dataset.state = state;
  stateBadge.textContent = text;
}

function render() {
  renderServers(
    entries,
    subscriptions,
    running,
    connectedId,
    targetEntry()?.id ?? null,
    pings,
    collapsed,
    showAddress,
  );
  renderHome();
}

/** Name without leaking the endpoint; used when a server has no label. */
function serverLabel(entry) {
  return entry.profile.name || `Server ${entry.id.slice(0, 6)}`;
}

function serverMeta(entry) {
  const parts = [entry.profile.protocol.toUpperCase()];
  if (entry.profile.security && entry.profile.security !== "none") {
    parts.push(entry.profile.security.toUpperCase());
  }
  if (entry.profile.network && entry.profile.network !== "tcp") {
    parts.push(entry.profile.network);
  }
  // Privacy: the endpoint shows up only when the user asked for it.
  if (showAddress) parts.push(`${entry.profile.server}:${entry.profile.port}`);
  return parts.join(" · ");
}

/** The server the Connect button acts on: live, last used, then the first. */
function targetEntry() {
  return (
    entryById(connectedId) ??
    entryById(lastId) ??
    entries.find((entry) => entry.profile.port > 0) ??
    entries[0] ??
    null
  );
}

function formatUptime(ms) {
  const total = Math.max(0, Math.floor(ms / 1000));
  const hours = String(Math.floor(total / 3600)).padStart(2, "0");
  const minutes = String(Math.floor((total % 3600) / 60)).padStart(2, "0");
  const seconds = String(total % 60).padStart(2, "0");
  return `${hours}:${minutes}:${seconds}`;
}

function renderUptime() {
  const since = statusInfo?.since;
  const show = running && typeof since === "number";
  homeUptime.hidden = !show;
  if (show) homeUptime.textContent = formatUptime(Date.now() - since);
}

/** Connection tab: state, server, session timer and the main button. */
function renderHome() {
  const entry = targetEntry();
  const connecting = stateBadge.dataset.state === "connecting";
  const failed = Boolean(connectError) && !running && !connecting;
  homeServer.textContent = entry ? serverLabel(entry) : "No server selected";
  homeMeta.textContent = entry
    ? serverMeta(entry)
    : "Add a subscription in the Servers tab.";
  // A failure gets its own error container; the server meta stays factual.
  homeError.hidden = !failed;
  if (failed) homeErrorReason.textContent = connectError;
  // The indicator only speaks about an ongoing process: a determinate wave
  // grows with the reported stage, a plain sweep covers the time before the
  // first stage lands, and a live session settles into a completed line.
  homeState.dataset.state = running
    ? "online"
    : connecting
      ? "connecting"
      : failed
        ? "failed"
        : "offline";
  homeState.textContent = running
    ? "connected"
    : connecting
      ? "connecting"
      : failed
        ? "failed"
        : "offline";
  const known = typeof connectProgress?.value === "number";
  homeWave.hidden = !running && !connecting;
  if (homeWave.hidden) {
    homeWave.dataset.phase = "connecting";
  } else if (running) {
    homeWave.dataset.phase = "connected";
    homeWave.style.setProperty("--p", "1");
  } else if (known) {
    homeWave.dataset.phase = "determinate";
    homeWave.style.setProperty("--p", String(connectProgress.value));
  } else {
    homeWave.dataset.phase = "connecting";
  }
  const percent = known ? ` ${Math.round(connectProgress.value * 100)}%` : "";
  homeConnect.textContent = running
    ? "Disconnect"
    : connecting
      ? `Connecting…${percent}`
      : "Connect";
  homeConnect.disabled = connecting;
  renderUptime();
}

function entryById(id) {
  return entries.find((entry) => entry.id === id);
}

async function reload({ probe = true } = {}) {
  [entries, subscriptions] = await Promise.all([
    invoke("profiles_list"),
    invoke("subscriptions_list"),
  ]);

  const known = new Set(entries.map((entry) => entry.id));
  for (const id of pings.keys()) {
    if (!known.has(id)) pings.delete(id);
  }

  render();
  if (probe) pingGroup(null);
}

/** Watch a live session so an unexpected stop shows up without a click. */
async function watchSession() {
  if (!running || busy) return;
  try {
    const status = await invoke("tunnel_status");
    if (!status.running) await refreshStatus();
  } catch {
    // The backend may still be shutting down; the next tick tries again.
  }
}

async function refreshStatus() {
  const status = await invoke("tunnel_status");
  statusInfo = status;
  running = status.running;
  if (running) {
    // A rebuilt window (closed to the tray, then reopened) starts empty: the
    // session is still up in the backend, so the last server is taken as the
    // live one instead of leaving every row idle.
    connectedId ??= lastId;
  } else {
    connectedId = null;
    connectProgress = null;
  }
  setState(running ? "online" : "offline", running ? "connected" : "offline");
  render();
}

/** Run one command at a time and surface failures in the log. */
async function guard(task, { connect = false } = {}) {
  if (busy) return;
  busy = true;
  try {
    await task();
  } catch (error) {
    setState("error", "error");
    appendLog(`error: ${error}`);
    if (connect) {
      // A toggle that silently snaps back is worse than no toggle: say what
      // went wrong and keep it on the card until the next attempt.
      connectError = connectFailure(error);
      // On the Status tab the error container already explains the failure;
      // a toast there would just repeat it over the Connect button.
      if (homeView?.dataset.active !== "true") showNotice(connectError, "error");
      renderHome();
    }
  } finally {
    busy = false;
    await refreshStatus().catch(() => {});
  }
}

/** Turns helper/Xray failure text into one readable line. */
function connectFailure(error) {
  const text = String(error ?? "").trim();
  if (/not pass traffic|does not pass/i.test(text)) {
    return "Server is not responding – try another one.";
  }
  if (/cancelled|canceled|dismissed/i.test(text)) return "Connection cancelled.";
  if (/timed out|timeout/i.test(text)) return "Connection timed out.";
  if (/unreachable|no route/i.test(text)) return "Network is unreachable.";
  if (/not installed|missing/i.test(text)) return "The tunnel helper is not installed.";
  return text.length > 160 ? `${text.slice(0, 160)}…` : text;
}

function paintPing(id) {
  const badge = serversRoot.querySelector(`[data-ping-id="${id}"]`);
  if (!badge) return;
  const state = pings.get(id);
  badge.textContent = pingLabel(state);
  badge.dataset.state = pingTone(state);
  badge.title = state?.error ?? `Measure ${pingKind.toUpperCase()} latency`;
}

/** Proxy probes boot a temporary Xray per server, so they run fewer at a time. */
function pingConcurrency() {
  return pingKind === "proxy" ? 3 : 6;
}

async function pingEntry(entry) {
  if (!entry || !entry.profile.server) return;
  if (pingKind !== "icmp" && !entry.profile.port) return;
  pings.set(entry.id, { pending: true });
  paintPing(entry.id);

  try {
    const ms = await invoke("ping_entry", { id: entry.id });
    pings.set(entry.id, { ms });
  } catch (error) {
    pings.set(entry.id, { error: String(error) });
  }
  paintPing(entry.id);
}

/**
 * Probe a group (`subscription` id, `"__saved"` for loose servers) or every
 * server when `null`, a few at a time, without blocking the list.
 */
async function pingGroup(subscription) {
  const generation = ++pingGeneration;
  const inGroup = (entry) => {
    if (subscription === null) return true;
    if (subscription === "__saved") return !entry.subscription;
    return entry.subscription === subscription;
  };

  const queue = entries.filter(
    (entry) =>
      inGroup(entry) &&
      entry.profile.server &&
      (pingKind === "icmp" || entry.profile.port > 0),
  );
  const worker = async () => {
    while (queue.length && generation === pingGeneration) {
      await pingEntry(queue.shift());
    }
  };
  const workers = Math.min(pingConcurrency(), queue.length);
  await Promise.all(Array.from({ length: workers }, worker));
}

/** A row click selects the server; a running session moves to it at once. */
async function selectEntry(entry) {
  if (entry.id === connectedId) return;
  connectError = null;
  lastId = entry.id;
  localStorage.setItem(LAST_KEY, entry.id);
  if (running) {
    await guard(() => connectEntry(entry.id), { connect: true });
    return;
  }
  render();
}

async function connectEntry(id) {
  connectProgress = null;
  connectError = null;
  setState("connecting", "connecting");
  renderHome(); // The fast loading wave appears before the round trip ends.
  const status = await invoke("connect_entry", { id });
  connectedId = id;
  lastId = id;
  localStorage.setItem(LAST_KEY, id);
  statusInfo = status;
  running = true;
  appendLog(
    status.mode === "tun"
      ? `tun ${status.interface} · socks5 :${status.socksPort} · http :${status.httpPort} · pid ${status.pid}`
      : `socks5 127.0.0.1:${status.socksPort} · http 127.0.0.1:${status.httpPort} · pid ${status.pid}`,
  );
  setState("online", "connected");
  render();
}

async function disconnect() {
  await invoke("disconnect");
  connectedId = null;
  statusInfo = null;
  running = false;
  connectProgress = null;
  connectError = null;
  appendLog("disconnected");
  setState("offline", "offline");
  render();
}

async function importSource(value) {
  if (/^(vless|vmess|trojan|ss):\/\//i.test(value)) {
    await invoke("profile_add", { link: value });
  } else {
    await invoke("subscription_add", { url: value });
  }
}

function renameEntry(entry) {
  askName({
    title: "Rename server",
    label: "Server name",
    value: entry.profile.name || entry.profile.server,
  }).then((name) => {
    if (name === null) return;
    guard(async () => {
      entries = await invoke("profile_rename", { id: entry.id, name });
      render();
    });
  });
}

function renameSubscription(id) {
  const subscription = subscriptions.find((item) => item.id === id);
  if (!subscription) return;
  askName({
    title: "Rename subscription",
    label: "Subscription name",
    value: subscription.title || subscription.url,
  }).then((title) => {
    if (title === null) return;
    guard(async () => {
      subscriptions = await invoke("subscription_rename", { id, title });
      render();
    });
  });
}

function togglePin(entry) {
  guard(async () => {
    entries = await invoke("profile_pin", { id: entry.id, pinned: !entry.pinned });
    render();
  });
}

function removeEntry(entry) {
  guard(async () => {
    if (connectedId === entry.id) await disconnect();
    entries = await invoke("profile_remove", { id: entry.id });
    await reload({ probe: false });
  });
}

function removeSubscription(id) {
  guard(async () => {
    const connected = entryById(connectedId);
    if (connected?.subscription === id) await disconnect();
    entries = await invoke("subscription_remove", { id });
    collateralForget(id);
    await reload({ probe: false });
  });
}

function collateralForget(id) {
  collapsed.delete(id);
  saveCollapsed();
}

function refreshSubscription(button, id) {
  guard(async () => {
    button.disabled = true;
    entries = await invoke("subscription_refresh", { id });
    subscriptions = await invoke("subscriptions_list");
    render();
    appendLog("subscription refreshed");
    pingGroup(id);
  });
}

function toggleCollapsed(target) {
  if (collapsed.has(target)) collapsed.delete(target);
  else collapsed.add(target);
  saveCollapsed();
  render();
}

function openEntryMenu(anchor, entry) {
  openMenu(
    anchor,
    [
      { label: "Test ping", action: "ping" },
      { label: "Rename", action: "rename" },
      { label: entry.pinned ? "Unpin" : "Pin to top", action: "pin" },
      { label: "Remove", action: "remove", danger: true },
    ],
    (action) => {
      if (action === "ping") pingEntry(entry);
      else if (action === "rename") renameEntry(entry);
      else if (action === "pin") togglePin(entry);
      else if (action === "remove") removeEntry(entry);
    },
  );
}

function openSubscriptionMenu(anchor, id) {
  openMenu(
    anchor,
    [
      { label: "Test ping", action: "ping" },
      { label: "Refresh", action: "refresh" },
      { label: "Rename", action: "rename" },
      { label: "Remove", action: "remove", danger: true },
    ],
    (action) => {
      if (action === "ping") pingGroup(id);
      else if (action === "refresh") refreshSubscription(anchor, id);
      else if (action === "rename") renameSubscription(id);
      else if (action === "remove") removeSubscription(id);
    },
  );
}

addSubButton.addEventListener("click", () =>
  guard(async () => {
    const value = subInput.value.trim();
    if (!value) return;
    addSubButton.disabled = true;
    addSubButton.textContent = "…";
    try {
      await importSource(value);
      subInput.value = "";
      await reload();
      appendLog("source imported");
    } catch (error) {
      appendLog(`import failed: ${error}`);
      throw error;
    } finally {
      addSubButton.disabled = false;
      addSubButton.textContent = "Add";
    }
  }),
);

serversRoot.addEventListener("click", (event) => {
  const button = event.target.closest("button");

  if (button) {
    const action = button.dataset.action;
    if (action === "collapse") {
      toggleCollapsed(button.dataset.target);
      return;
    }
    if (action === "ping-all") {
      pingGroup(button.dataset.subscription);
      return;
    }
    const subscription = button.dataset.subscription;
    if (action === "refresh") {
      refreshSubscription(button, subscription);
      return;
    }
    if (action === "remove-sub") {
      removeSubscription(subscription);
      return;
    }
    if (action === "sub-menu") {
      openSubscriptionMenu(button, subscription);
      return;
    }
  }

  // The list only picks the server; connecting lives on the status tab.
  const item = event.target.closest(".item");
  if (!item) return;
  const entry = entryById(item.dataset.id);
  if (!entry) return;

  const action = button?.dataset.action;
  if (action === "remove") {
    removeEntry(entry);
  } else if (action === "ping") {
    pingEntry(entry);
  } else if (action === "entry-menu") {
    openEntryMenu(button, entry);
  } else {
    selectEntry(entry);
  }
});

window.addEventListener("error", (event) => {
  appendLog(`js error: ${event.message}`);
});

listen("xray-log", (event) => appendLog(event.payload));

/** The backend reports real connect stages; the wave is determinate meanwhile. */
listen("tunnel-progress", (event) => {
  const value = event.payload?.value;
  if (typeof value !== "number") return;
  connectProgress = { value, label: event.payload.label ?? "" };
  if (stateBadge.dataset.state === "connecting" || running) renderHome();
});

/** The backend fuse stopped a session the user did not stop. */
listen("tunnel-ended", (event) => {
  const mode = event.payload?.mode === "tun" ? "TUN" : "proxy";
  showNotice(`Connection lost (${mode}) – settings restored`);
  connectProgress = null;
  refreshStatus().catch(() => {});
});

window.addEventListener("rumurule:ping-settings", (event) => {
  pingKind = event.detail.pingKind;
  pings.clear();
  render();
  pingGroup(null);
});

/** The privacy switch changed; repaint without re-probing. */
window.addEventListener("rumurule:show-address", (event) => {
  showAddress = event.detail === true;
  render();
});

async function start() {
  try {
    const settings = await invoke("settings_get");
    pingKind = settings.pingKind;
    showAddress = settings.showAddress === true;
  } catch (error) {
    appendLog(`settings error: ${error}`);
  }
  await reload();
  await refreshStatus();
  setInterval(watchSession, 2000);
  setInterval(renderUptime, 1000);
  await initWelcome();

  homeConnect.addEventListener("click", () =>
    guard(async () => {
      if (running) {
        await disconnect();
        return;
      }
      const entry = targetEntry();
      if (!entry) {
        appendLog("no server selected");
        return;
      }
      await connectEntry(entry.id);
    }, { connect: true }),
  );

  homeErrorDismiss.addEventListener("click", () => {
    connectError = null;
    renderHome();
  });
}

initTheme();
initMotion();
initTabs();
initSettings();
start().catch((error) => appendLog(`error: ${error}`));
