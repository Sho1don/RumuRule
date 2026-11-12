// Material You dynamic color: derive a tonal palette from the desktop accent.

import { invoke, listen } from "./api.js";

const FALLBACK_ACCENT = "#4f6d8f";

const root = document.documentElement;
const darkQuery = window.matchMedia("(prefers-color-scheme: dark)");

let accent = FALLBACK_ACCENT;
let systemDark = null;

function parseHex(hex) {
  const value = hex.replace("#", "");
  return [
    parseInt(value.slice(0, 2), 16),
    parseInt(value.slice(2, 4), 16),
    parseInt(value.slice(4, 6), 16),
  ];
}

function toHsl([r, g, b]) {
  const rn = r / 255;
  const gn = g / 255;
  const bn = b / 255;
  const max = Math.max(rn, gn, bn);
  const min = Math.min(rn, gn, bn);
  const l = (max + min) / 2;

  if (max === min) return [0, 0, l];

  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  let h;
  if (max === rn) h = ((gn - bn) / d + (gn < bn ? 6 : 0)) / 6;
  else if (max === gn) h = ((bn - rn) / d + 2) / 6;
  else h = ((rn - gn) / d + 4) / 6;

  return [h * 360, s, l];
}

const hsl = (h, s, l) => `hsl(${h.toFixed(1)} ${(s * 100).toFixed(1)}% ${(l * 100).toFixed(1)}%)`;

function palette(hex, dark) {
  const [h, rawS] = toHsl(parseHex(hex));
  const s = Math.min(Math.max(rawS, 0.28), 0.72);

  if (dark) {
    return {
      "--md-primary": hsl(h, s * 0.85, 0.78),
      "--md-on-primary": hsl(h, s, 0.16),
      "--md-primary-container": hsl(h, s * 0.55, 0.3),
      "--md-on-primary-container": hsl(h, s * 0.8, 0.9),
      "--md-secondary-container": hsl(h, s * 0.35, 0.26),
      "--md-on-secondary-container": hsl(h, s * 0.6, 0.9),
      "--md-surface": hsl(h, 0.12, 0.075),
      "--md-surface-container": hsl(h, 0.1, 0.13),
      "--md-surface-container-high": hsl(h, 0.09, 0.17),
      "--md-on-surface": hsl(h, 0.1, 0.9),
      "--md-on-surface-variant": hsl(h, 0.08, 0.78),
      "--md-outline": hsl(h, 0.08, 0.55),
      "--md-outline-variant": hsl(h, 0.1, 0.3),
      "--md-error": "#ffb4ab",
    };
  }

  return {
    "--md-primary": hsl(h, s, 0.38),
    "--md-on-primary": "#ffffff",
    "--md-primary-container": hsl(h, s * 0.55, 0.9),
    "--md-on-primary-container": hsl(h, s * 0.8, 0.15),
    "--md-secondary-container": hsl(h, s * 0.3, 0.9),
    "--md-on-secondary-container": hsl(h, s * 0.6, 0.18),
    "--md-surface": hsl(h, 0.35, 0.985),
    "--md-surface-container": hsl(h, 0.22, 0.93),
    "--md-surface-container-high": hsl(h, 0.18, 0.9),
    "--md-on-surface": hsl(h, 0.15, 0.12),
    "--md-on-surface-variant": hsl(h, 0.08, 0.35),
    "--md-outline": hsl(h, 0.05, 0.48),
    "--md-outline-variant": hsl(h, 0.12, 0.78),
    "--md-error": "#ba1a1a",
  };
}

function apply() {
  const dark = systemDark ?? darkQuery.matches;
  root.dataset.theme = dark ? "dark" : "light";
  for (const [name, value] of Object.entries(palette(accent, dark))) {
    root.style.setProperty(name, value);
  }
}

/** Load the desktop palette and follow later changes. */
export function initTheme() {
  apply();

  invoke("system_theme")
    .then((theme) => {
      if (!theme) return;
      accent = theme.accent;
      systemDark = theme.dark;
      apply();
    })
    .catch(() => {
      // Desktop theme is optional: fall back to the media query.
    });

  listen("system-theme", (event) => {
    if (!event.payload) return;
    accent = event.payload.accent;
    systemDark = event.payload.dark;
    apply();
  });

  darkQuery.addEventListener("change", apply);
}
