/** @typedef {import('$lib/themes/registry.js').ThemeId} ThemeId */

import { createHighlighterCore } from 'shiki/core'
import { createOnigurumaEngine } from 'shiki/engine/oniguruma'
import { DEFAULT_THEME_ID, shikiThemeId } from '$lib/themes/registry.js'

/**
 * The grammars a code block can be highlighted in. `shiki`'s full bundle
 * imports all ~290 of them, so every one was built into the app (8.7 MB of
 * chunks) for code blocks that are nearly always SQL, JSON, a shell command or
 * one of a few programming languages. Each is still fetched only the first
 * time a block in it shows up; anything else renders as plain text.
 * @type {Record<string, () => Promise<any>>}
 */
const LANGS = {
  sql: () => import('shiki/langs/sql.mjs'),
  json: () => import('shiki/langs/json.mjs'),
  jsonc: () => import('shiki/langs/jsonc.mjs'),
  javascript: () => import('shiki/langs/javascript.mjs'),
  typescript: () => import('shiki/langs/typescript.mjs'),
  jsx: () => import('shiki/langs/jsx.mjs'),
  tsx: () => import('shiki/langs/tsx.mjs'),
  python: () => import('shiki/langs/python.mjs'),
  shellscript: () => import('shiki/langs/shellscript.mjs'),
  powershell: () => import('shiki/langs/powershell.mjs'),
  yaml: () => import('shiki/langs/yaml.mjs'),
  toml: () => import('shiki/langs/toml.mjs'),
  ini: () => import('shiki/langs/ini.mjs'),
  dotenv: () => import('shiki/langs/dotenv.mjs'),
  markdown: () => import('shiki/langs/markdown.mjs'),
  html: () => import('shiki/langs/html.mjs'),
  css: () => import('shiki/langs/css.mjs'),
  xml: () => import('shiki/langs/xml.mjs'),
  graphql: () => import('shiki/langs/graphql.mjs'),
  prisma: () => import('shiki/langs/prisma.mjs'),
  dockerfile: () => import('shiki/langs/dockerfile.mjs'),
  diff: () => import('shiki/langs/diff.mjs'),
  rust: () => import('shiki/langs/rust.mjs'),
  go: () => import('shiki/langs/go.mjs'),
  java: () => import('shiki/langs/java.mjs'),
  kotlin: () => import('shiki/langs/kotlin.mjs'),
  csharp: () => import('shiki/langs/csharp.mjs'),
  php: () => import('shiki/langs/php.mjs'),
  ruby: () => import('shiki/langs/ruby.mjs'),
  swift: () => import('shiki/langs/swift.mjs'),
  c: () => import('shiki/langs/c.mjs'),
  lua: () => import('shiki/langs/lua.mjs'),
}

/** Fence names that mean one of the grammars above. */
const ALIASES = /** @type {Record<string, string>} */ ({
  js: 'javascript', mjs: 'javascript', cjs: 'javascript', node: 'javascript',
  ts: 'typescript', mts: 'typescript',
  py: 'python', python3: 'python',
  sh: 'shellscript', bash: 'shellscript', shell: 'shellscript', zsh: 'shellscript', console: 'shellscript',
  ps1: 'powershell', pwsh: 'powershell',
  yml: 'yaml', md: 'markdown', htm: 'html', svg: 'xml', gql: 'graphql', docker: 'dockerfile', env: 'dotenv',
  json5: 'jsonc', jsonl: 'json',
  rs: 'rust', golang: 'go', kt: 'kotlin', cs: 'csharp', 'c#': 'csharp', rb: 'ruby', h: 'c',
  postgres: 'sql', postgresql: 'sql', psql: 'sql', pgsql: 'sql', plpgsql: 'sql', mysql: 'sql', mariadb: 'sql',
  sqlite: 'sql', tsql: 'sql', mssql: 'sql', plsql: 'sql', ddl: 'sql',
})

/** Bundled Shiki themes - aligned with app.css (vitesse-light / vitesse-dark). */
const THEMES = [() => import('shiki/themes/vitesse-light.mjs'), () => import('shiki/themes/vitesse-dark.mjs')]

// Only SQL is preloaded: it is in nearly every block. Each TextMate grammar is
// a large JSON blob parsed on the main thread, so loading the whole set up
// front cost ~75ms before the first code block could paint; with one it is
// ~1ms. Everything else is fetched by `ensureLang` the first time it is needed.
const PRELOAD_LANG_IDS = ['sql']

/** @type {ReturnType<typeof createHighlighterCore> | null} */
let highlighterPromise = null

/** In-flight/completed on-demand grammar loads, so N blocks share one fetch. */
const langLoads = new Map()

function loadHighlighter() {
  if (!highlighterPromise) {
    highlighterPromise = createHighlighterCore({
      themes: THEMES.map((load) => load()),
      langs: PRELOAD_LANG_IDS.map((id) => LANGS[id]()),
      engine: createOnigurumaEngine(import('shiki/wasm')),
    })
  }
  return highlighterPromise
}

/**
 * Make sure `lang`'s grammar is loaded before highlighting with it.
 * @param {Awaited<ReturnType<typeof createHighlighterCore>>} highlighter
 * @param {string} lang
 */
async function ensureLang(highlighter, lang) {
  if (highlighter.getLoadedLanguages().includes(lang)) return
  const load = LANGS[lang]
  if (!load) return
  let pending = langLoads.get(lang)
  if (!pending) {
    // Swallow failures - `highlightCode` falls back to plaintext.
    pending = highlighter.loadLanguage(load()).catch(() => {})
    langLoads.set(lang, pending)
  }
  await pending
}

/** @param {string} [lang] */
export function resolveShikiLang(lang) {
  const id = String(lang ?? '')
    .toLowerCase()
    .trim()
  const normalized = ALIASES[id] ?? id
  if (normalized in LANGS) return normalized
  return 'plaintext'
}

/**
 * @param {string} code
 * @param {string} [lang]
 * @param {ThemeId} [theme]
 */
export async function highlightCode(code, lang, theme = DEFAULT_THEME_ID) {
  const highlighter = await loadHighlighter()
  const resolved = resolveShikiLang(lang)
  const shikiTheme = shikiThemeId(theme)
  await ensureLang(highlighter, resolved)
  try {
    return highlighter.codeToHtml(code, {
      lang: resolved,
      theme: shikiTheme,
    })
  } catch {
    return highlighter.codeToHtml(code, {
      lang: 'plaintext',
      theme: shikiTheme,
    })
  }
}
