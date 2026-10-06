/**
 * Editor colour presets as CodeMirror extensions: a theme for the chrome plus
 * a highlight style for the code. The palettes live in themes/editor-themes.js.
 *
 * The editor's own theme (CodeEditor.svelte) and the themes surfaces add to it
 * are written against the app's tokens: --foreground, --muted-foreground,
 * --border, --popover, the --json-* colours. A preset redefines those tokens
 * on the editor root, so every one of those rules (find panel, completion
 * list, fold markers, lint and quick-fix tooltips, statement bands) takes the
 * preset's colours without a rule of its own. Tooltips render outside the
 * editor but carry its theme classes, so they pick the tokens up too.
 * --destructive, --warning, --success and --primary stay the app's: errors,
 * search hits and actions read the same in every preset.
 */
import { EditorView } from '@codemirror/view'
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language'
import { tags as t } from '@lezer/highlight'
import { EDITOR_THEMES } from '$lib/themes/editor-themes.js'

/** Built once per preset, so every open editor shares one set of style rules. */
const built = new Map()

/**
 * Theme and highlighting for a preset, or null for 'app' (the editor keeps
 * its own token-based colours).
 * @param {import('$lib/themes/editor-themes.js').EditorThemeId} id
 * @returns {import('@codemirror/state').Extension | null}
 */
export function editorThemeExtension(id) {
  const def = EDITOR_THEMES[id]
  if (!def?.palette) return null
  let ext = built.get(id)
  if (!ext) {
    ext = [chrome(def.palette, def.dark), syntaxHighlighting(highlightStyle(def.palette))]
    built.set(id, ext)
  }
  return ext
}

/**
 * @param {import('$lib/themes/editor-themes.js').EditorPalette} p
 * @param {boolean} dark
 */
function chrome(p, dark) {
  return EditorView.theme(
    {
      '&': {
        '--background': p.bg,
        '--foreground': p.fg,
        '--muted': p.line,
        '--muted-foreground': p.muted,
        '--border': p.border,
        '--popover': p.panel,
        '--popover-foreground': p.fg,
        // Derived tokens resolve where they are declared (the app root), so
        // the find field's hairline is derived again from the preset here.
        '--field-border': 'color-mix(in oklch, var(--foreground) 30%, var(--background))',
        // The completion list's kind dots and any JSON-coloured mark.
        '--json-key': p.property,
        '--json-string': p.string,
        '--json-number': p.number,
        '--json-boolean': p.keyword,
        '--json-null': p.comment,
      },
      // The editor itself only; '&' alone also matches the tooltip host.
      '&.cm-editor': { backgroundColor: p.bg, color: p.fg },
      '.cm-content': { caretColor: p.cursor },
      '.cm-cursor, .cm-dropCursor': { borderLeftColor: p.cursor },
      '.cm-selectionBackground': {
        backgroundColor: `color-mix(in srgb, ${p.selection} 65%, transparent) !important`,
      },
      '&.cm-focused .cm-selectionBackground': { backgroundColor: `${p.selection} !important` },
      '.cm-activeLine': { backgroundColor: p.line },
      '.cm-gutters': { color: p.gutter },
      '.cm-activeLineGutter': { color: p.gutterActive },
    },
    { dark },
  )
}

/**
 * Token colours. Tags are the ones the languages in use emit: lang-sql gives
 * keyword, typeName, standard(name) for builtins, name for identifiers,
 * special(string) for quoted identifiers, bool, null, number, string,
 * operator, punctuation and the comment kinds; JSON, JavaScript and HTML add
 * propertyName, variableName, function(...), definition(...), className,
 * tagName, attributeName and attributeValue.
 * @param {import('$lib/themes/editor-themes.js').EditorPalette} p
 */
function highlightStyle(p) {
  return HighlightStyle.define([
    { tag: t.keyword, color: p.keyword },
    { tag: [t.string, t.attributeValue, t.regexp], color: p.string },
    // A quoted identifier is a name, not a string ("name" = 'ad').
    { tag: t.special(t.string), color: p.variable },
    { tag: t.number, color: p.number },
    { tag: [t.bool, t.null, t.atom, t.escape, t.special(t.name), t.constant(t.name), t.constant(t.variableName)], color: p.constant },
    { tag: t.comment, color: p.comment, fontStyle: 'italic' },
    { tag: [t.standard(t.name), t.function(t.variableName), t.function(t.propertyName), t.macroName], color: p.fn },
    { tag: [t.typeName, t.className, t.namespace, t.standard(t.typeName)], color: p.type },
    { tag: t.operator, color: p.operator },
    { tag: [t.name, t.variableName, t.labelName, t.definition(t.variableName)], color: p.variable },
    { tag: [t.propertyName, t.attributeName, t.definition(t.propertyName)], color: p.property },
    { tag: [t.punctuation, t.bracket, t.angleBracket, t.separator], color: p.punctuation },
    { tag: t.tagName, color: p.tag },
    { tag: [t.meta, t.annotation], color: p.fn },
    { tag: t.invalid, textDecoration: 'underline wavy' },
  ])
}
