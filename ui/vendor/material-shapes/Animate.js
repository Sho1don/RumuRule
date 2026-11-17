/*
 * A tiny, dependency-free animator for morphing between two shapes.
 *
 * It builds a Morph once (feature matching is the expensive part) and samples
 * it every animation frame, handing you an SVG path string you can drop onto a
 * <path d="..."> or a CSS `clip-path: path(...)`.
 */
import { Morph } from './Morph.js';
import { morphToPath } from './MaterialShapes.js';
import { resolveEasing } from './Easing.js';
const now = typeof performance !== 'undefined' && typeof performance.now === 'function'
    ? () => performance.now()
    : () => Date.now();
const raf = typeof requestAnimationFrame === 'function'
    ? (cb) => requestAnimationFrame(cb)
    : (cb) => setTimeout(() => cb(now()), 16);
const cancelRaf = typeof cancelAnimationFrame === 'function'
    ? (h) => cancelAnimationFrame(h)
    : (h) => clearTimeout(h);
/**
 * Animates a morph between two shapes (or two ends of a prebuilt Morph),
 * calling `onFrame` with an SVG path string each frame.
 *
 * @example
 * animateMorph(MaterialShapes.Circle, MaterialShapes.Heart, {
 *   duration: 400,
 *   easing: 'emphasized',
 *   onFrame: (d) => pathEl.setAttribute('d', d),
 * });
 */
export function animateMorph(from, to, options) {
    let morph;
    let opts;
    if (from instanceof Morph) {
        morph = from;
        opts = to;
    }
    else {
        morph = new Morph(from, to);
        opts = options;
    }
    const duration = opts.duration ?? 400;
    const easing = resolveEasing(opts.easing);
    const reverse = opts.reverse ?? false;
    let handle = 0;
    let cancelled = false;
    let resolveFinished;
    const finished = new Promise((res) => (resolveFinished = res));
    const render = (progress) => {
        const p = reverse ? 1 - progress : progress;
        opts.onFrame(morphToPath(morph, easing(p)).toSvgPathData(), easing(p));
    };
    if (duration <= 0) {
        render(1);
        opts.onComplete?.();
        resolveFinished();
        return { cancel() { }, finished };
    }
    const start = now();
    const tick = (t) => {
        if (cancelled)
            return;
        const elapsed = t - start;
        const progress = Math.min(1, elapsed / duration);
        render(progress);
        if (progress < 1) {
            handle = raf(tick);
        }
        else {
            opts.onComplete?.();
            resolveFinished();
        }
    };
    handle = raf(tick);
    return {
        cancel() {
            if (cancelled)
                return;
            cancelled = true;
            cancelRaf(handle);
            resolveFinished();
        },
        finished,
    };
}
//# sourceMappingURL=Animate.js.map