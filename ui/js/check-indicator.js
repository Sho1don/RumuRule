// Material 3 Expressive loading indicator: one solid shape rotates while it
// morphs through the seven shapes from the Material shape library
// (SoftBurst, Cookie9Sided, Pentagon, Pill, Sunny, Cookie4Sided, Oval).
//
// The shapes and the morphing come from material-shapes-ts, a port of
// AndroidX Graphics Shapes; see ui/vendor/material-shapes for the license.
import { MaterialShapes, Morph, morphToPath } from "../vendor/material-shapes/index.js";

// Motion values from androidx.compose.material3 LoadingIndicator.kt.
const MORPH_INTERVAL = 650; // one shape per interval
const FULL_TURN = 4666; // continuous background rotation
const QUARTER_TURN = 90; // extra turn added by each morph
// Active indicator as a share of its 48 px container, with the animation
// scale AndroidX applies so rotating shapes never clip.
const SHAPE_SCALE = 0.6852541768130452;
const BOX = 48;
const SPRING_DAMPING = 0.6;
const SPRING_STIFFNESS = 200;

const SEQUENCE = [
  MaterialShapes.SoftBurst,
  MaterialShapes.Cookie9Sided,
  MaterialShapes.Pentagon,
  MaterialShapes.Pill,
  MaterialShapes.Sunny,
  MaterialShapes.Cookie4Sided,
  MaterialShapes.Oval,
];

// Building a morph is expensive, so the seven links are made once.
const MORPHS = SEQUENCE.map(
  (shape, index) => new Morph(shape, SEQUENCE[(index + 1) % SEQUENCE.length]),
);

const rotor = document.querySelector("#check-overlay .check-indicator-rotor");
const active = document.querySelector("#check-overlay .check-indicator-shape");
const reduced = window.matchMedia("(prefers-reduced-motion: reduce)");

let frame = 0;
let startedAt = 0;
let running = false;

/** Compose's spring(dampingRatio = 0.6, stiffness = 200) as 0 → 1. */
function spring(ms) {
  const omega = Math.sqrt(SPRING_STIFFNESS);
  const damped = omega * Math.sqrt(1 - SPRING_DAMPING * SPRING_DAMPING);
  const t = ms / 1000;
  const decay = Math.exp(-SPRING_DAMPING * omega * t);
  const wave = Math.cos(damped * t) + ((SPRING_DAMPING * omega) / damped) * Math.sin(damped * t);
  return 1 - decay * wave;
}

function drawPath(index, progress) {
  active.setAttribute("d", morphToPath(MORPHS[index], progress).toSvgPathData());
}

function paint(now) {
  if (!running) return;
  const elapsed = now - startedAt;
  const index = Math.floor(elapsed / MORPH_INTERVAL) % SEQUENCE.length;
  const inside = elapsed % MORPH_INTERVAL;
  const progress = spring(inside);
  const turn = QUARTER_TURN * (index + 1) + QUARTER_TURN * progress;
  const global = (elapsed / FULL_TURN) * 360;

  drawPath(index, progress);
  rotor.setAttribute(
    "transform",
    `translate(${BOX / 2} ${BOX / 2}) rotate(${turn + global}) ` +
      `scale(${BOX * SHAPE_SCALE}) translate(-0.5 -0.5)`,
  );
  frame = requestAnimationFrame(paint);
}

/** Resting first shape for users who ask the system for less motion. */
function paintStill() {
  drawPath(0, 0);
  rotor.setAttribute(
    "transform",
    `translate(${BOX / 2} ${BOX / 2}) rotate(${QUARTER_TURN}) ` +
      `scale(${BOX * SHAPE_SCALE}) translate(-0.5 -0.5)`,
  );
}

export function startCheckIndicator() {
  if (running) return;
  if (reduced.matches) {
    paintStill();
    return;
  }
  running = true;
  startedAt = performance.now();
  frame = requestAnimationFrame(paint);
}

export function stopCheckIndicator() {
  running = false;
  cancelAnimationFrame(frame);
  frame = 0;
}
