import path from 'path'
import { fileURLToPath } from 'url'
import { defineConfig } from 'vitest/config'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import tailwindcss from '@tailwindcss/vite'

const __dirname = path.dirname(fileURLToPath(import.meta.url))

// Component smoke tests: mount real components in a DOM and drive them once.
// They exist because two crashes reached a Nightly that a single render would
// have caught (a component used without its import, an effect that wrote state
// it read and looped until effect_update_depth_exceeded). Kept apart from the
// node unit tests so those stay fast and need no compiler.
export default defineConfig({
  plugins: [svelte({ hot: false }), tailwindcss()],
  test: {
    environment: 'jsdom',
    include: ['src/**/*.dom.test.js'],
    setupFiles: ['src/test/dom-setup.js'],
    // The first test compiles the shared import tree.
    testTimeout: 30_000,
  },
  resolve: {
    conditions: ['browser'],
    alias: {
      $lib: path.resolve(__dirname, './src/lib'),
    },
  },
})
