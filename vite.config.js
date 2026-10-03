import path from 'path'
import { fileURLToPath } from 'url'
import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import tailwindcss from '@tailwindcss/vite'

const __dirname = path.dirname(fileURLToPath(import.meta.url))

// Workaround: @tailwindcss/vite (enforce:'pre') intercepts Svelte virtual CSS modules
// (?svelte&type=style&lang.css) before vite-plugin-svelte can populate the compiled CSS.
// When the parent component isn't in the module graph yet, Svelte's loadCompiledCss
// returns undefined, Vite falls back to reading the raw .svelte file, and Tailwind
// tries to parse Svelte source as CSS -> "Invalid declaration" error.
//
// Fix: pre-compile the parent .svelte module so the CSS meta is available, then guard
// the transform path so raw Svelte source never reaches Tailwind's CSS parser.
/** @type {import('vite').Plugin} */
const svelteVirtualCssFix = {
  name: 'svelte-virtual-css-fix',
  enforce: 'pre',
  async load(id) {
    if (!/\.svelte\?[^#]*type=style/.test(id)) return
    const svelteFile = id.split('?')[0]
    if (!this.getModuleInfo(svelteFile)?.meta?.svelte?.css) {
      try { await this.load({ id: svelteFile }) } catch {}
    }
    return null // hand off to vite-plugin-svelte:load-compiled-css
  },
  transform(code, id) {
    // Safety net: if Vite still served raw Svelte source as a CSS module,
    // return empty CSS so Tailwind's parser doesn't blow up.
    if (/\.svelte\?[^#]*type=style/.test(id) && /<script[\s>]/i.test(code)) {
      return { code: '' }
    }
  },
}

// https://vite.dev/config/
export default defineConfig({
  plugins: [
    svelteVirtualCssFix,
    svelte({ compilerOptions: { runes: true } }),
    tailwindcss(),
  ],
  optimizeDeps: {
    // canvas-confetti is only reached through a lazy `await import()` on the license
    // pages, so Vite's initial scan misses it. Discovering it mid-session re-runs the
    // optimizer, rehashes every pre-bundled chunk, and the webview then 404s on the
    // old runtime-*/index-client-*/Icon-* paths -> the whole app goes blank.
    include: [
      'canvas-confetti',
      // The ER diagram's layout engine, reached only through the lazily loaded
      // page; CommonJS, so it has to be pre-bundled before the page asks for it.
      'elkjs/lib/elk-api.js',
      // src/lib/monaco.js composes the editor entry by hand out of subpaths, and it
      // is itself only reached through a lazy import, so the same scan misses all of
      // these. Keep this list in step with the imports at the top of that file.
      'monaco-editor/esm/vs/editor/edcore.main.js',
      'monaco-editor/esm/vs/basic-languages/sql/sql.contribution.js',
      'monaco-editor/esm/vs/basic-languages/javascript/javascript.contribution.js',
      'monaco-editor/esm/vs/basic-languages/typescript/typescript.contribution.js',
      'monaco-editor/esm/vs/basic-languages/rust/rust.contribution.js',
      'monaco-editor/esm/vs/basic-languages/markdown/markdown.contribution.js',
      'monaco-editor/esm/vs/language/json/monaco.contribution.js',
      'monaco-editor/esm/vs/language/typescript/monaco.contribution.js',
    ],
  },
  resolve: {
    alias: {
      $lib: path.resolve(__dirname, './src/lib'),
    },
  },
  server: {
    // Must match `devUrl` in src-tauri/tauri.conf.json. strictPort so a busy port
    // fails loudly instead of sliding to 8081 and leaving the webview on a dead URL.
    port: 1420,
    strictPort: true,
    // The dev server may resolve node_modules from a parent dir (e.g. when running
    // from a git worktree that shares the repo's node_modules). Relax the fs
    // allow-list so those assets (fonts, etc.) serve instead of being blocked.
    // Dev-only — has no effect on `tauri build` / production.
    fs: { strict: false },
    // Watch the frontend, not the repo. With no ignore list the watcher crawled
    // src-tauri/target (Rust build output, 100+ GB here) and website/, holding
    // ~3 GB and a steady share of a core just to track them, then processing
    // thousands of change events on every cargo rebuild - the lag spikes while
    // the app recompiled. Tauri's own template ignores src-tauri for this reason;
    // `tauri dev` watches the Rust side itself.
    watch: {
      ignored: [
        '**/src-tauri/**',
        '**/website/**',
        '**/dist/**',
        '**/bucket/**',
        '**/docker/**',
        '**/docs/**',
        '**/examples/**',
        '**/packaging/**',
      ],
    },
  },
  build: {
    chunkSizeWarningLimit: 1500,
  },
})
