/**
 * Props a test can change after mounting: $state is only allowed in .svelte
 * and .svelte.js files, and the DOM tests are plain .js.
 * @template {Record<string, any>} T
 * @param {T} initial
 * @returns {T}
 */
export function reactiveProps(initial) {
  const props = $state(initial)
  return props
}
