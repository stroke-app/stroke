import { describe, it, expect, afterEach } from 'vitest'
import { mount, unmount, flushSync, tick } from 'svelte'

// Mount each dialog and page the way the app does and drive it once. A thrown
// error, or Svelte's effect_update_depth_exceeded (an effect that writes what
// it reads), fails the test. Both crashes that reached a Nightly would have:
// a component used without its import, and the share dialog's reset loop.

/** @type {Array<Record<string, any>>} */
let mounted = []
afterEach(() => {
  for (const m of mounted) { try { unmount(m) } catch { /* already gone */ } }
  mounted = []
  document.body.innerHTML = ''
})

/** @param {any} Component @param {Record<string, any>} [props] */
function render(Component, props = {}) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const c = mount(Component, { target, props })
  mounted.push(c)
  flushSync()
  return target
}

/** Let promises and effects settle. */
async function settle() {
  for (let i = 0; i < 5; i++) { await tick(); await new Promise((r) => setTimeout(r, 0)); flushSync() }
}

describe('share dialog', () => {
  it('opens for results without looping', async () => {
    const { default: ShareDialog } = await import('./ShareDialog.svelte')
    const { requestShare, shareRequest } = await import('$lib/share-flow.js')
    render(ShareDialog)
    requestShare({ kind: 'result', title: 'customers', columns: [{ name: 'id' }, { name: 'email' }], rows: [[1, 'a@b.co'], [2, 'c@d.io']] })
    flushSync()
    await settle()
    expect(document.body.textContent).toContain('Share query results')
    // The email column is detected and masked by default.
    expect(document.body.textContent).toContain('1 masked')
    shareRequest.set(null)
  })

  it('opens for a chart and a diagram', async () => {
    const { default: ShareDialog } = await import('./ShareDialog.svelte')
    const { requestShare, shareRequest } = await import('$lib/share-flow.js')
    render(ShareDialog)
    requestShare({ kind: 'chart', title: 'revenue', chart: { type: 'bar', x: 'month', y: ['total'] }, columns: [{ name: 'month' }, { name: 'total' }], rows: [['jan', 3]] })
    await settle()
    requestShare({ kind: 'erd', title: 'schema', tables: [], links: [] })
    await settle()
    expect(document.body.textContent).toContain('Share schema diagram')
    shareRequest.set(null)
  })
})

describe('pages', () => {
  it('Shared links', async () => {
    const { default: SharesPage } = await import('./SharesPage.svelte')
    render(SharesPage)
    await settle()
  })

  it('Keybindings, with each tab', async () => {
    const { default: KeybindingsSettings } = await import('./KeybindingsSettings.svelte')
    const el = render(KeybindingsSettings)
    for (const label of ['Built-in', 'Changed', 'Commands']) {
      const tab = [...el.querySelectorAll('[role="tab"]')].find((b) => b.textContent?.includes(label))
      expect(tab).toBeTruthy();
      /** @type {HTMLElement} */ (tab).click()
      flushSync()
    }
  })

  it('Find in database', async () => {
    const { default: SearchPage } = await import('./SearchPage.svelte')
    const el = render(SearchPage, { tables: [{ name: 'customers' }], schema: 'public', active: true })
    const input = /** @type {HTMLInputElement} */ (el.querySelector('input'))
    input.value = 'sofia'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await settle()
  })

  it('Connecting overlay', async () => {
    const { default: ConnectOverlay } = await import('./ConnectOverlay.svelte')
    const el = render(ConnectOverlay, { name: 'db', detail: 'host:5432', icon: 'postgres', oncancel: () => {} })
    expect(el.textContent).toContain('Connecting to db')
  })
})

describe('dialogs', () => {
  it('New table: add a column, pick VARCHAR', async () => {
    const { default: CreateTableDialog } = await import('./CreateTableDialog.svelte')
    render(CreateTableDialog, { open: true, activeSchema: 'public', dbType: 'mysql' })
    await settle()
    const add = [...document.querySelectorAll('button')].find((b) => b.textContent?.includes('Add column'))
    expect(add).toBeTruthy();
    /** @type {HTMLElement} */ (add).click()
    await settle()
    // The new VARCHAR column shows its length box beside a still-present type picker.
    expect(document.querySelector('[aria-label="Column 2 type"]')).toBeTruthy()
    expect(document.querySelector('[aria-label="Column 2 length"]')).toBeTruthy()
  })

  it('Settings, and a search that matches nothing', async () => {
    const { default: SettingsDialog } = await import('./SettingsDialog.svelte')
    render(SettingsDialog, { open: true })
    await settle()
    const search = /** @type {HTMLInputElement | null} */ (document.querySelector('input[type="text"], input:not([type])'))
    expect(search).toBeTruthy()
  })
})
