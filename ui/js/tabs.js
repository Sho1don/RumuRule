// Floating section dock: one visible view at a time, switchable by swipe.

const STORAGE_KEY = "rumurule.tab";
const SCROLL_KEY = "rumurule.scroll";
const DEFAULT_TAB = "home";

// Drag shorter than this just bounces back; a quick flick counts as a swipe.
const SWIPE_MIN = 48;
const FLICK_DISTANCE = 24;
const FLICK_TIME = 280;

const tabs = [...document.querySelectorAll("[data-tab]")];
const names = tabs.map((tab) => tab.dataset.tab);
const app = document.querySelector(".app");

let drag = null;

function viewOf(name) {
  return document.querySelector(`#view-${name}`);
}

function activeTab() {
  return tabs.find((tab) => tab.getAttribute("aria-selected") === "true")?.dataset.tab;
}

/** Tab `step` positions away from `name`, or null at the ends. */
function adjacent(name, step) {
  const index = names.indexOf(name) + step;
  return index >= 0 && index < names.length ? names[index] : null;
}

function playSlide(name, step) {
  playClass(name, step > 0 ? "slide-from-right" : "slide-from-left", 300);
}

function playFade(name) {
  playClass(name, "fade-through", 300);
}

/** Add `effect` for one run, then drop it so a later visit replays cleanly. */
function playClass(name, effect, fallbackMs) {
  // Clear leftovers: a view hidden mid-animation never fires animationend,
  // so its class would replay on the next visit.
  for (const tab of tabs) {
    viewOf(tab.dataset.tab).classList.remove(
      "slide-from-right",
      "slide-from-left",
      "fade-through",
    );
  }

  const view = viewOf(name);
  void view.offsetWidth;
  view.classList.add(effect);

  const clear = () => view.classList.remove(effect);
  view.addEventListener("animationend", clear, { once: true });
  // Fallback for animations cancelled by a hidden view.
  setTimeout(clear, fallbackMs);
}

/** Scroll offsets per section, so a rebuilt window comes back where the last
    one was: the tray closes the window instead of hiding it, and the process
    that holds the DOM is stopped. */
const scrollMemory = readScrolls();
let scrollTimer = 0;

function readScrolls() {
  try {
    return JSON.parse(localStorage.getItem(SCROLL_KEY) || "{}");
  } catch {
    return {};
  }
}

function saveScroll(name, top) {
  scrollMemory[name] = top;
  // Writes go out in batches; the Rust side waits out the last flush before
  // the web process is stopped.
  clearTimeout(scrollTimer);
  scrollTimer = setTimeout(() => {
    try {
      localStorage.setItem(SCROLL_KEY, JSON.stringify(scrollMemory));
    } catch {
      // Storage can be disabled; positions then last for this view only.
    }
  }, 150);
}

/** Applies the stored offset, retrying while the section is still empty. */
function restoreScroll(name, tries = 0) {
  const view = viewOf(name);
  const top = scrollMemory[name];
  if (!view || !top) return;
  if (view.dataset.active !== "true" || tries > 40) return;
  view.scrollTop = top;
  // A list that has not rendered yet clamps the offset to zero.
  if (Math.abs(view.scrollTop - top) > 1) {
    requestAnimationFrame(() => restoreScroll(name, tries + 1));
  }
}

/** Show one view and mark its tab as selected. */
function activate(name, { persist = true, slide = 0, fade = false } = {}) {
  if (!names.includes(name)) return;

  for (const tab of tabs) {
    const selected = tab.dataset.tab === name;
    tab.setAttribute("aria-selected", String(selected));
    tab.tabIndex = selected ? 0 : -1;
    viewOf(tab.dataset.tab).dataset.active = String(selected);
  }

  if (persist) {
    try {
      localStorage.setItem(STORAGE_KEY, name);
    } catch {
      // Storage can be disabled; the dock still works without it.
    }
  }

  restoreScroll(name);

  // A drag carries direction: shared axis X. A plain click just surfaces the
  // view as fade-through; the initial restore animates nothing.
  if (slide) playSlide(name, slide);
  else if (fade) playFade(name);
  else viewOf(name).classList.remove("slide-from-right", "slide-from-left", "fade-through");

  document.dispatchEvent(new CustomEvent("viewchange", { detail: name }));
}

function releaseView(view) {
  if (!view) return;
  view.style.transition = "";
  void view.offsetWidth;
  view.style.transform = "";
}

function startDrag(event) {
  if (event.pointerType === "mouse" && event.button !== 0) return;
  if (event.target.closest("input, textarea, select, button, a, label, pre, code")) {
    return;
  }

  drag = {
    id: event.pointerId,
    x: event.clientX,
    y: event.clientY,
    time: performance.now(),
    name: activeTab(),
    view: viewOf(activeTab()),
    axis: null,
  };
}

function moveDrag(event) {
  if (!drag || event.pointerId !== drag.id || !drag.view) return;

  const dx = event.clientX - drag.x;
  const dy = event.clientY - drag.y;

  if (!drag.axis) {
    if (Math.abs(dx) < 8 && Math.abs(dy) < 8) return;
    // Vertical movement belongs to the scrolling list, not to the swipe.
    drag.axis = Math.abs(dx) > Math.abs(dy) * 1.2 ? "x" : "y";
    if (drag.axis === "y") {
      drag = null;
      return;
    }
    drag.view.style.transition = "none";
  }

  const step = dx < 0 ? 1 : -1;
  // Resist dragging past the first or last section.
  const factor = adjacent(drag.name, step) ? 0.38 : 0.1;
  drag.view.style.transform = `translateX(${dx * factor}px)`;
}

function endDrag(event, cancelled = false) {
  if (!drag) return;
  const { id, name, view, x, time, axis } = drag;
  drag = null;
  if (event.pointerId !== id) return;

  releaseView(view);
  if (cancelled || axis !== "x") return;

  const dx = event.clientX - x;
  const elapsed = performance.now() - time;
  const flick = Math.abs(dx) > FLICK_DISTANCE && elapsed < FLICK_TIME;
  const passed = Math.abs(dx) > Math.min(90, Math.max(SWIPE_MIN, window.innerWidth * 0.1)) || flick;
  if (!passed) return;

  const step = dx < 0 ? 1 : -1;
  const target = adjacent(name, step);
  if (target) activate(target, { slide: step });
}

/** Wire the dock, swipes and keyboard, then restore the last section. */
export function initTabs() {
  let saved = null;
  try {
    saved = localStorage.getItem(STORAGE_KEY);
  } catch {
    // Ignore unavailable storage.
  }

  activate(saved && names.includes(saved) ? saved : DEFAULT_TAB, { persist: false });

  for (const tab of tabs) {
    tab.addEventListener("click", () => activate(tab.dataset.tab));
    const view = viewOf(tab.dataset.tab);
    view.addEventListener(
      "scroll",
      () => {
        if (view.dataset.active === "true") saveScroll(tab.dataset.tab, view.scrollTop);
      },
      { passive: true },
    );
  }

  app.addEventListener("pointerdown", startDrag);
  window.addEventListener("pointermove", moveDrag, { passive: true });
  window.addEventListener("pointerup", (event) => endDrag(event));
  window.addEventListener("pointercancel", (event) => endDrag(event, true));

  // Arrow keys move between tabs, as expected for a tab list.
  document.addEventListener("keydown", (event) => {
    if (!event.target.matches("[data-tab]")) return;
    const step = event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
    if (!step) return;
    event.preventDefault();
    const index = names.indexOf(event.target.dataset.tab);
    const next = tabs[(index + step + tabs.length) % tabs.length];
    activate(next.dataset.tab, { slide: step });
    next.focus();
  });
}
