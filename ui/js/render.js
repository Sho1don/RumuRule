// Rendering of the grouped server list.

const countLabel = document.querySelector("#count");
const serversRoot = document.querySelector("#servers");
const serversEmpty = document.querySelector("#servers-empty");

const ICONS = {
  refresh: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
    stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
    <path d="M20 11a8 8 0 1 0-2.3 5.7"></path>
    <path d="M20 5v6h-6"></path>
  </svg>`,
  ping: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
    stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
    <path d="M3 12h3l2.5-6 4 12L15 12h6"></path>
  </svg>`,
  more: `<svg viewBox="0 0 24 24" fill="currentColor" stroke="none">
    <circle cx="12" cy="5" r="1.7"></circle>
    <circle cx="12" cy="12" r="1.7"></circle>
    <circle cx="12" cy="19" r="1.7"></circle>
  </svg>`,
  chevron: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
    stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
    <path d="M6 9l6 6 6-6"></path>
  </svg>`,
  pin: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
    stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
    <path d="M9 4h6l-1 6 3 3H7l3-3-1-6z"></path>
    <path d="M12 13v7"></path>
  </svg>`,
  check: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
    stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
    <path d="M4.5 12.5 9.5 17.5 19.5 6.5"></path>
  </svg>`,
};

function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

/** Extended details of a server; the endpoint stays hidden unless asked. */
function describe(profile, showAddress) {
  const parts = [profile.protocol.toUpperCase()];
  if (profile.security && profile.security !== "none") parts.push(profile.security.toUpperCase());
  if (profile.network && profile.network !== "tcp") parts.push(profile.network);
  if (showAddress) parts.push(`${profile.server}:${profile.port}`);
  return parts.join(" · ");
}

export function pingLabel(state) {
  if (!state) return "–";
  if (state.skipped) return "udp";
  if (state.pending) return "…";
  if (state.ms !== undefined) return `${state.ms} ms`;
  return "fail";
}

export function pingTone(state) {
  if (!state) return "idle";
  if (state.skipped) return "idle";
  if (state.pending) return "pending";
  if (state.ms !== undefined) {
    if (state.ms < 200) return "fast";
    if (state.ms < 500) return "ok";
    return "slow";
  }
  return "fail";
}

function iconButton(action, icon, title, { id, subscription, danger = false } = {}) {
  const button = el("button", danger ? "icon-button danger" : "icon-button");
  button.type = "button";
  button.dataset.action = action;
  if (id) button.dataset.id = id;
  if (subscription !== undefined) button.dataset.subscription = subscription;
  button.title = title;
  button.setAttribute("aria-label", title);
  button.innerHTML = ICONS[icon];
  return button;
}

function pingButton(entry, pings) {
  const state = pings.get(entry.id);
  const button = el("button", "ping", pingLabel(state));
  button.type = "button";
  button.dataset.action = "ping";
  button.dataset.pingId = entry.id;
  button.dataset.state = pingTone(state);
  button.title = state?.error ?? "Measure TCP latency";
  return button;
}

function serverItem(entry, running, connectedId, selectedId, pings, showAddress) {
  const active = running && connectedId === entry.id;
  const selected = entry.id === selectedId;
  const item = el("li", "item");
  item.dataset.id = entry.id;
  item.dataset.active = String(active);
  item.dataset.selected = String(selected);
  item.dataset.pinned = String(entry.pinned === true);

  const main = el("div", "item-main");
  const title = el("span", "item-title-row");
  if (selected) {
    // A check makes the current pick obvious even at a glance.
    const check = el("span", "item-check");
    check.innerHTML = ICONS.check;
    check.title = "Selected";
    title.append(check);
  }
  if (entry.pinned) {
    const pin = el("span", "pin-mark");
    pin.innerHTML = ICONS.pin;
    pin.title = "Pinned";
    title.append(pin);
  }
  title.append(el("span", "item-title", nameOf(entry)));
  if (active) title.append(el("span", "item-live", "Live"));
  main.append(title, el("span", "item-meta", describe(entry.profile, showAddress)));

  const buttons = el("div", "item-actions");
  buttons.append(
    pingButton(entry, pings),
    iconButton("entry-menu", "more", "Server options", { id: entry.id }),
  );

  item.append(main, buttons);
  return item;
}

/** Regional indicators, tags, variation selectors and joiners of a flag emoji. */
const FLAG = /^[\u{1F1E6}-\u{1F1FF}\u{1F3F4}\u{E0020}-\u{E007F}\u{FE0F}\u{200D}]+/u;
/** Servers marked with a star (bypass nodes) go above the plain ones. */
const STAR = /[\u2B50\u2605]/u;

function nameOf(entry) {
  return entry.profile.name || `Server ${entry.id.slice(0, 6)}`;
}

/** Name without the leading flag, so countries compare alphabetically. */
function countryKey(entry) {
  return nameOf(entry).replace(FLAG, "").trim();
}

function sortRank(entry) {
  if (entry.pinned) return 0;
  return STAR.test(nameOf(entry)) ? 1 : 2;
}

function serverList(entries, running, connectedId, selectedId, pings, showAddress) {
  const list = el("ul", "list");
  // Pinned first in subscription order, then starred nodes, then countries
  // alphabetically. Array#sort is stable, so equal ranks keep their order.
  const ordered = [...entries].sort((a, b) => {
    const byRank = sortRank(a) - sortRank(b);
    if (byRank !== 0 || sortRank(a) === 0) return byRank;
    return countryKey(a).localeCompare(countryKey(b), "ru");
  });
  for (const entry of ordered) {
    list.append(serverItem(entry, running, connectedId, selectedId, pings, showAddress));
  }
  return list;
}

function groupHeader({ target, title, subtitle, count, collapsed, subscription }) {
  const header = el("div", "group-header");

  const toggle = el("button", "group-toggle");
  toggle.type = "button";
  toggle.dataset.action = "collapse";
  toggle.dataset.target = target;
  toggle.setAttribute("aria-expanded", String(!collapsed));
  const chevron = el("span", "group-chevron");
  chevron.innerHTML = ICONS.chevron;
  toggle.append(chevron, el("span", "group-title", title), el("span", "group-count", String(count)));
  if (subtitle) toggle.title = subtitle;

  const actions = el("div", "group-actions");
  actions.append(iconButton("ping-all", "ping", "Test every server", { subscription: target }));
  if (subscription) {
    actions.append(
      iconButton("refresh", "refresh", "Refresh subscription", { subscription: target }),
      iconButton("sub-menu", "more", "Subscription options", { subscription: target }),
    );
  }

  header.append(toggle, actions);
  return header;
}

/** Render servers grouped under the subscription that provided them. */
export function renderServers(
  entries,
  subscriptions,
  running,
  connectedId,
  selectedId,
  pings,
  collapsed,
  showAddress,
) {
  serversRoot.textContent = "";
  countLabel.textContent = `${entries.length} saved`;
  serversEmpty.hidden = entries.length > 0;

  const grouped = new Map(subscriptions.map((subscription) => [subscription.id, []]));
  const standalone = [];
  for (const entry of entries) {
    if (entry.subscription && grouped.has(entry.subscription)) {
      grouped.get(entry.subscription).push(entry);
    } else {
      standalone.push(entry);
    }
  }

  for (const subscription of subscriptions) {
    const members = grouped.get(subscription.id);
    const isCollapsed = collapsed.has(subscription.id);
    const group = el("section", "group");
    group.dataset.subscription = subscription.id;
    group.dataset.collapsed = String(isCollapsed);
    group.append(
      groupHeader({
        target: subscription.id,
        title: subscription.title || subscription.url,
        subtitle: subscription.url,
        count: members.length,
        collapsed: isCollapsed,
        subscription,
      }),
    );

    if (members.length) {
      const list = serverList(members, running, connectedId, selectedId, pings, showAddress);
      list.hidden = isCollapsed;
      group.append(list);
    } else if (!isCollapsed) {
      group.append(el("p", "muted empty", "No servers in this subscription."));
    }
    serversRoot.append(group);
  }

  if (standalone.length) {
    const isCollapsed = collapsed.has("__saved");
    const group = el("section", "group");
    group.dataset.collapsed = String(isCollapsed);
    group.append(
      groupHeader({
        target: "__saved",
        title: "Saved servers",
        count: standalone.length,
        collapsed: isCollapsed,
      }),
    );
    const list = serverList(standalone, running, connectedId, selectedId, pings, showAddress);
    list.hidden = isCollapsed;
    group.append(list);
    serversRoot.append(group);
  }
}
