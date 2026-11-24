/**
 * The UI type scale, resolved to whole pixels.
 *
 * Kept apart from the settings store so it stays a pure function of the root
 * size - the store pulls in browser-only deps, and this is the piece worth
 * testing.
 */

/**
 * Root font size at 100%, on every platform.
 *
 * 14px matches native desktop UI density (DESIGN_SYSTEM.md: text-ui = 14px,
 * text-ui-sm = 13px, text-ui-xs = 12px, text-ui-2xs = 11px, text-ui-3xs = 10px)
 * while keeping every rung distinct on a whole-pixel grid down to 90%.
 */
export const ROOT_BASE_PX = 14

/**
 * UI zoom scale (font + layout). 1 = 100%.
 *
 * Rungs are spaced so each one lands on its own whole-pixel root at 14px
 * within 3pp of its advertised label, and the lowest rung (90%, 13px) keeps
 * the smallest step at 9px with every step strictly ordered.
 */
export const ZOOM_STEPS = [0.9, 1, 1.1, 1.2, 1.35, 1.5, 1.7, 2]

/** Root size for a zoom rung. Whole pixels: WebKit rasterises fractional text soft. */
export const rootPxFor = (zoom) => Math.max(1, Math.round(ROOT_BASE_PX * zoom))

/** `[step, nominal px at a 14px root, optional line-height]`. */
export const UI_TYPE_SCALE = [
  ['3xl', 24, 30],
  ['2xl', 20, 28],
  ['xl', 18, 26],
  ['lg', 16, 24],
  ['base', 14],
  ['15', 15],
  ['sm', 13],
  ['xs', 12],
  ['2xs', 11],
  ['3xs', 10],
]

/** The scale's own reference root - every nominal size above is quoted at 14px. */
export const TYPE_SCALE_REF = 14

/** Steps ordered small -> large, so the separation walk below has a direction. */
const TYPE_SCALE_ASC = [...UI_TYPE_SCALE].sort((a, b) => a[1] - b[1])
const TYPE_SCALE_BASE_I = TYPE_SCALE_ASC.findIndex(([step]) => step === 'base')

/**
 * Resolve the UI type scale to whole pixels for a given root size.
 *
 * Rounding each step on its own collapses neighbours at small roots - at a 12px
 * root both `3xs` (10) and `2xs` (11) round to 9px, and two tokens that render
 * identically cannot express the hierarchy the scale exists for. So round first,
 * then walk outward from `base` forcing at least a pixel between neighbours.
 *
 * `base` is pinned to the root itself rather than rounded, because the canvas
 * grid sizes its text from `--app-font-size`; letting the two drift by a pixel
 * would show up as the grid and the DOM disagreeing about the same type step.
 *
 * @param {number} rootPx
 * @returns {Array<[string, number]>} `[step, px]`, ordered small -> large.
 */
export function buildTypeScale(rootPx) {
  const px = TYPE_SCALE_ASC.map(([, size]) =>
    Math.max(1, Math.round((size * rootPx) / TYPE_SCALE_REF)),
  )
  px[TYPE_SCALE_BASE_I] = rootPx
  for (let i = TYPE_SCALE_BASE_I + 1; i < px.length; i++) {
    px[i] = Math.max(px[i], px[i - 1] + 1)
  }
  for (let i = TYPE_SCALE_BASE_I - 1; i >= 0; i--) {
    px[i] = Math.max(1, Math.min(px[i], px[i + 1] - 1))
  }
  return TYPE_SCALE_ASC.map(([step], i) => [step, px[i]])
}
