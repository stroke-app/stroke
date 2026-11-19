import path from 'path'
import { fileURLToPath } from 'url'
import { defineConfig } from 'vitest/config'

const __dirname = path.dirname(fileURLToPath(import.meta.url))

// Pure-logic unit tests run in a Node environment — no Svelte compilation or DOM
// needed for the modules under test (table-query, table-list, studio-tabs, format-sql).
// The `$lib` alias must mirror vite.config.js so imported modules resolve here too.
export default defineConfig({
  test: {
    environment: 'node',
    include: ['src/**/*.test.js'],
    // Component smoke tests need a DOM and the compiler: vitest.dom.config.js.
    exclude: ['src/**/*.dom.test.js', '**/node_modules/**'],
  },
  resolve: {
    alias: {
      $lib: path.resolve(__dirname, './src/lib'),
    },
  },
})
