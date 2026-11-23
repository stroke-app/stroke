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

  it('Find in database drops its results when the connection changes', async () => {
    // Kept across connections, the page showed one database's matches on another.
    const bridge = /** @type {any} */ (globalThis).__TAURI_INTERNALS__
    const invoke = bridge.invoke
    bridge.invoke = async (/** @type {string} */ cmd) => {
      if (cmd === 'pg_get_table_rows') return { columns: [{ name: 'name', dataType: 'text' }], rows: [['Hammer']], total: 1, primaryKey: [] }
      return null
    }
    try {
      const { default: SearchPage } = await import('./SearchPage.svelte')
      const target = document.createElement('div')
      document.body.appendChild(target)
      const { reactiveProps } = await import('../../test/reactive-props.svelte.js')
      const props = reactiveProps({ tables: [{ name: 'products' }], schema: 'shop', active: true, connectionId: 'a' })
      const c = mount(SearchPage, { target, props })
      mounted.push(c)
      await settle()
      const input = /** @type {HTMLInputElement} */ (target.querySelector('input'))
      input.value = 'ham'
      input.dispatchEvent(new Event('input', { bubbles: true }))
      input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
      await settle(); await new Promise((r) => setTimeout(r, 400)); await settle()
      expect(target.textContent).toContain('Hammer')
      props.connectionId = 'b'
      props.tables = [{ name: 'people' }]
      props.schema = 'public'
      await settle()
      expect(target.textContent).not.toContain('Hammer')
    } finally {
      bridge.invoke = invoke
    }
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

  it('Import data: choosing a file parses it without a state write in a derived', async () => {
    // The parse ran in a $derived that also assigned parseError; Svelte stops
    // the whole UI on that (state_unsafe_mutation) as soon as a file is chosen.
    const bridge = /** @type {any} */ (globalThis).__TAURI_INTERNALS__
    const invoke = bridge.invoke
    bridge.invoke = async (/** @type {string} */ cmd) => {
      if (cmd === 'plugin:dialog|open') return '/data/people.csv'
      if (cmd === 'read_file') return 'id,name\n1,"Lovelace, Ada"\n2,Zoë\n'
      return null
    }
    try {
      const { default: ImportDataDialog } = await import('./ImportDataDialog.svelte')
      render(ImportDataDialog, { open: true, schema: 'public', table: 'people', columns: [{ name: 'id', dataType: 'int4' }, { name: 'name', dataType: 'text' }], primaryKey: ['id'] })
      await settle()
      const choose = [...document.querySelectorAll('button')].find((b) => b.textContent?.includes('Choose a file'))
      expect(choose).toBeTruthy();
      /** @type {HTMLElement} */ (choose).click()
      await settle()
      expect(document.body.textContent).toContain('people.csv')
      expect(document.body.textContent).toContain('Import 2 rows')
    } finally {
      bridge.invoke = invoke
    }
  })

  it('Settings, and a search that matches nothing', async () => {
    const { default: SettingsDialog } = await import('./SettingsDialog.svelte')
    render(SettingsDialog, { open: true })
    await settle()
    const search = /** @type {HTMLInputElement | null} */ (document.querySelector('input[type="text"], input:not([type])'))
    expect(search).toBeTruthy()
  })
})

describe('pages changed overnight', () => {
  it('Data diff: Compare waits until both sides name a table', async () => {
    const { default: DataDiffPage } = await import('./DataDiffPage.svelte')
    render(DataDiffPage, {
      schemas: ['public'], tables: [{ name: 'users' }], activeSchema: 'public',
      connections: [{ id: 'c1', name: 'local', type: 'postgres' }], currentConnectionId: 'c1',
    })
    await settle()
    expect(document.body.textContent).toContain('Compare any two data sources')
    const compare = [...document.querySelectorAll('button')].find((b) => b.textContent?.trim() === 'Compare')
    expect(compare?.hasAttribute('disabled')).toBe(true)
  })

  it('Tab bar with many tabs', async () => {
    const { default: TabBar } = await import('./TabBar.svelte')
    const tabs = Array.from({ length: 30 }, (_, i) => ({ id: `t${i}`, kind: 'table', title: `table_${i}` }))
    const el = render(TabBar, { tabs, activeTabId: 't29' })
    await settle()
    expect(el.querySelectorAll('[role="tab"]').length).toBe(30)
  })

  it('Query plan from a Postgres EXPLAIN', async () => {
    const { default: ExplainPlan } = await import('./ExplainPlan.svelte')
    const plan = [{ Plan: { 'Node Type': 'Limit', 'Total Cost': 2.5, 'Plan Rows': 5, Plans: [{ 'Node Type': 'Seq Scan', 'Relation Name': 'pings', 'Total Cost': 2, 'Plan Rows': 100 }] } }]
    render(ExplainPlan, { result: { plan, driver: 'postgres' }, analyze: false })
    await settle()
    expect(document.body.textContent).toContain('Estimated')
  })

  it('Dashboard with nothing on it', async () => {
    const { default: DashboardPage } = await import('./DashboardPage.svelte')
    render(DashboardPage, { connection: { id: 'c1', name: 'local', type: 'postgres' }, active: true })
    await settle()
  })
})

describe('updates', () => {
  const releases = [
    { version: '2.10.8', date: '2026-11-15', sections: [{ title: 'New Features', group: 'New Features', type: 'feature', items: ['Smoke tests gate Nightly'] }] },
    { version: '2.10.7', date: '2026-11-15', sections: [{ title: 'Changes', group: 'Changes', type: 'change', items: ['Inset tree rows'] }] },
  ]

  it("What's new lists each version on its own", async () => {
    const { default: WhatsNewDialog } = await import('./WhatsNewDialog.svelte')
    render(WhatsNewDialog, { open: true, current: '2.10.8-nightly.202611150842', previous: '2.10.6-nightly.1', releases })
    await settle()
    const text = document.body.textContent ?? ''
    // The short version with a Nightly badge; the full build is its tooltip.
    expect(text).toContain('Stroke 2.10.8 Nightly')
    expect(document.querySelector('[title="2.10.8-nightly.202611150842"]')).not.toBeNull()
    expect(text).toContain('2.10.7')
    expect(text).toContain('2 changes across 2 versions')
  })

  it('Settings opens the switch dialog for the other track', async () => {
    const { default: SettingsDialog } = await import('./SettingsDialog.svelte')
    render(SettingsDialog, { open: true })
    await settle()
    expect(document.body.textContent).toContain('Update track')
  })
})

