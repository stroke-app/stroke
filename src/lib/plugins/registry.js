// Extension registry. Built-in extensions register here, and third-party plugins
// installed from a folder feed the same hooks from a Worker sandbox - see
// ./external/host.js. Built-ins run in-process and answer synchronously;
// external ones answer from the host's directive cache, one frame behind, which
// is why the contract has always been plain serializable data.
//
// Hooks:
//  - formatter:  { appliesTo(type, name, value), format(value, type, config, name, ctx)
//                  -> render-directive | null }
//  - linkify:    { linkify(value, type, name, config) -> href | null }
//  - generators: [{ id, label, generate() -> string }]
//  - transforms: [{ id, label, appliesTo(value, type, name), run(value, type, name) -> string }]
//  - annotator:  toggle only (rendering lives in the table)
//
// A render-directive is a plain object; fields are merged across formatters
// (first writer wins per field) so e.g. money `display` and heatmap `bgTint`
// stack on one cell. Recognized fields:
//   display, title, badge{bg,fg}, swatch, dot, fg, bgTint, mask, reveal, link, warn,
//   trail{text, color?: 'success'|'destructive'} (muted text after the value)
// ctx: { stats, row, prev (the row drawn above), colIdx, colIndex (name → index), fk }
import { isPluginEnabled, getPluginConfig } from '$lib/stores/plugins.js'
import { externalFormatCell, externalFormattersActive } from './external/host.js'
import { betterTime } from './extensions/better-time.js'
import { freshness } from './extensions/freshness.js'
import { nullishValues } from './extensions/nullish-values.js'
import { numberFormat } from './extensions/number-format.js'
import { moneyFormat } from './extensions/money-format.js'
import { durationFormat } from './extensions/duration-format.js'
import { statusBadge } from './extensions/status-badge.js'
import { booleanGlyph } from './extensions/boolean-glyph.js'
import { colorSwatch } from './extensions/color-swatch.js'
import { maskSensitive } from './extensions/mask-sensitive.js'
import { smartText } from './extensions/smart-text.js'
import { heatmap } from './extensions/heatmap.js'
import { validators } from './extensions/validators.js'
import { linkify } from './extensions/linkify.js'
import { columnAnnotator } from './extensions/column-annotator.js'
import { idGenerators } from './extensions/id-generators.js'
import { dataGen } from './extensions/data-gen.js'
import { cellTransforms } from './extensions/cell-transforms.js'
import { savedViews, findReplace } from './extensions/workflow.js'
import { fkLabels } from './extensions/fk-labels.js'
import { idDecoder } from './extensions/id-decoder.js'
import { tokenDecoder } from './extensions/token-decoder.js'
import { byteSize } from './extensions/byte-size.js'
import { ipAddress } from './extensions/ip-address.js'
import { duplicates } from './extensions/duplicates.js'
import { rowChange } from './extensions/row-change.js'
import { geoPair } from './extensions/geo-pair.js'

// Display order also = merge precedence for formatters (earlier wins per field).
export const EXTENSIONS = [
  nullishValues,
  betterTime,
  freshness,
  moneyFormat,
  numberFormat,
  durationFormat,
  statusBadge,
  booleanGlyph,
  colorSwatch,
  maskSensitive,
  smartText,
  byteSize,
  fkLabels,
  idDecoder,
  tokenDecoder,
  ipAddress,
  geoPair,
  rowChange,
  duplicates,
  heatmap,
  validators,
  linkify,
  columnAnnotator,
  idGenerators,
  dataGen,
  cellTransforms,
  savedViews,
  findReplace,
]

const FORMATTERS = EXTENSIONS.filter((e) => e.kind === 'formatter')
const LINKIFIERS = EXTENSIONS.filter((e) => e.kind === 'linkify')

/** IDs of formatter extensions that require per-column stats (heatmap, etc.). */
export const STATS_EXTENSION_IDS = EXTENSIONS.filter((e) => e.needsStats).map((e) => e.id)

/** @param {Record<string, unknown>} out @param {Record<string, unknown>} d */
function mergeDirective(out, d) {
  if (!d) return out
  if (!out) out = {}
  for (const k in d) {
    if (out[k] === undefined && d[k] !== undefined) out[k] = d[k]
  }
  return out
}

/**
 * Resolve a render directive for a scalar cell via enabled formatters + linkifiers.
 * Hot path: called per visible cell on every canvas repaint - allocation only
 * happens for cells that actually match an enabled extension.
 * @param {unknown} value
 * @param {string} type
 * @param {string} name
 * @param {{ stats?: object } | undefined} [ctx]
 * @returns {Record<string, unknown> | null}
 */
export function formatCellValue(value, type, name, ctx) {
  /** @type {Record<string, unknown> | null} */
  let out = null
  for (const ext of FORMATTERS) {
    if (!isPluginEnabled(ext.id)) continue
    if (ext.appliesTo && !ext.appliesTo(type, name, value)) continue
    const d = ext.format(value, type, getPluginConfig(ext.id), name, ctx)
    if (d) out = mergeDirective(out, d)
  }
  for (const ext of LINKIFIERS) {
    if (!isPluginEnabled(ext.id)) continue
    const href = ext.linkify(value, type, name, getPluginConfig(ext.id))
    if (href) { out = mergeDirective(out, { link: href }); break }
  }
  // Built-ins win every field they set: an installed plugin adds to the picture
  // rather than overriding what ships in the app.
  const ext = externalFormatCell(value, type, name)
  if (ext) out = mergeDirective(out, ext)
  return out
}

/**
 * Compute a linkifier href for a cell (used by the click handler).
 * @param {unknown} value @param {string} type @param {string} name
 * @returns {string | null}
 */
export function linkifyValue(value, type, name) {
  for (const ext of LINKIFIERS) {
    if (!isPluginEnabled(ext.id)) continue
    const href = ext.linkify(value, type, name, getPluginConfig(ext.id))
    if (href) return href
  }
  return null
}

/** Whether any stats-dependent extension is enabled (gates stats computation). */
export function statsNeeded() {
  return STATS_EXTENSION_IDS.some((id) => isPluginEnabled(id))
}

/**
 * Whether any formatter or linkifier is enabled. Lets the renderer skip the
 * per-cell formatCellValue() loop entirely in the common case (no display
 * extensions on) - zero added cost on the scroll hot path.
 */
export function anyDisplayExtEnabled() {
  for (const ext of FORMATTERS) if (isPluginEnabled(ext.id)) return true
  for (const ext of LINKIFIERS) if (isPluginEnabled(ext.id)) return true
  return externalFormattersActive()
}

/** Whether the column annotator strip should render. */
export function annotatorEnabled() {
  return isPluginEnabled('column-annotator')
}

/**
 * Transforms applicable to a given cell value, from enabled transform extensions.
 *
 * `informational` rides along because the caller decides what to do with the
 * result from it: a transform that yields a replacement value is staged as an
 * edit, one that yields a report about the value is copied and shown.
 * @param {unknown} value @param {string} type @param {string} name
 */
export function transformsFor(value, type, name) {
  /** @type {{ id: string, label: string, columnOnly: boolean, informational: boolean, run: (v: unknown) => string }[]} */
  const out = []
  for (const ext of EXTENSIONS) {
    if (ext.kind !== 'transforms' || !isPluginEnabled(ext.id)) continue
    for (const t of ext.transforms) {
      try {
        if (t.appliesTo(value, type, name)) {
          out.push({
            id: t.id,
            label: t.label,
            columnOnly: !!t.columnOnly,
            informational: !!t.informational,
            run: (v) => t.run(v, type, name),
          })
        }
      } catch {}
    }
  }
  return out
}

/**
 * Look up a single transform by id (for per-column transforms, which apply the
 * same transform to every cell). Returns the raw transform with its 3-arg run.
 * @param {string} id
 * @returns {{ id: string, label: string, appliesTo: Function, run: (v: unknown, type: string, name: string) => string } | null}
 */
export function transformById(id) {
  for (const ext of EXTENSIONS) {
    if (ext.kind !== 'transforms') continue
    for (const t of ext.transforms) if (t.id === id) return t
  }
  return null
}

/** Generators from all enabled generator extensions. */
export function enabledGenerators() {
  /** @type {{ id: string, label: string, generate: () => string }[]} */
  const out = []
  for (const ext of EXTENSIONS) {
    if (ext.kind !== 'generators' || !isPluginEnabled(ext.id)) continue
    out.push(...ext.generators)
  }
  return out
}

/**
 * Generators from enabled generator extensions, grouped into sections by each
 * generator's `group` (falling back to the extension name) - so the menu can
 * render "IDs", "Time", "Numbers", … as separate labelled sections.
 * @returns {{ group: string, items: { id: string, label: string, hint?: string, generate: () => string }[] }[]}
 */
export function enabledGeneratorGroups() {
  /** @type {{ group: string, items: any[] }[]} */
  const groups = []
  const byGroup = new Map()
  for (const ext of EXTENSIONS) {
    if (ext.kind !== 'generators' || !isPluginEnabled(ext.id)) continue
    for (const g of ext.generators) {
      const key = g.group || ext.name
      let bucket = byGroup.get(key)
      if (!bucket) { bucket = { group: key, items: [] }; byGroup.set(key, bucket); groups.push(bucket) }
      bucket.items.push(g)
    }
  }
  return groups
}
