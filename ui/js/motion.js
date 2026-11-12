// Drop decorative motion when the compositor runs slowly.
//
// WebKitGTK halves its frame rate to 30 FPS while the desktop is in the
// power-saver profile. Animations look choppy there, so state changes become
// instant until the frame rate recovers.

const root = document.documentElement;
const SAMPLE_FRAMES = 30;
const SLOW_FRAME_MS = 22;

function measure() {
  return new Promise((resolve) => {
    const deltas = [];
    let last = performance.now();

    const frame = (now) => {
      deltas.push(now - last);
      last = now;
      if (deltas.length < SAMPLE_FRAMES) {
        requestAnimationFrame(frame);
        return;
      }
      const sorted = [...deltas].sort((a, b) => a - b);
      resolve(sorted[Math.floor(sorted.length / 2)]);
    };

    requestAnimationFrame(frame);
  });
}

async function update() {
  if (document.hidden) return;
  const median = await measure();
  root.dataset.motion = median > SLOW_FRAME_MS ? "reduced" : "full";
}

/** Measure after startup and whenever the window comes back, plus periodically. */
export function initMotion() {
  setTimeout(update, 1200);
  setInterval(update, 10000);
  window.addEventListener("focus", update);
  document.addEventListener("visibilitychange", update);
}
