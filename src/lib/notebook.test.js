import { describe, expect, test } from 'vitest'
import {
  createNotebook,
  createSqlCell,
  createMarkdownCell,
  serializeNotebook,
  deserializeNotebook,
  substituteVars,
  variableNames,
  importNotebook,
  cellsFromMarkdown,
  cellsFromSql,
  cellsFromIpynb,
  toMarkdown,
  toSqlScript,
  toIpynb,
  toHtml,
} from './notebook.js'

describe('variables', () => {
  test('fills {{ name }} and reports the unknown ones', () => {
    const r = substituteVars("select * from t where status = '{{status}}' limit {{ n }} -- {{ other }}", [
      { name: 'status', value: 'paid' },
      { name: 'n', value: '10' },
    ])
    expect(r.sql).toBe("select * from t where status = 'paid' limit 10 -- {{ other }}")
    expect(r.missing).toEqual(['other'])
    expect(variableNames('{{a}} {{ b }} {{a}}')).toEqual(['a', 'b'])
  })
  test('round-trips through .sqlnb', () => {
    const nb = createNotebook('x')
    nb.variables = [{ name: 'a', value: '1' }]
    expect(deserializeNotebook(serializeNotebook(nb)).variables).toEqual([{ name: 'a', value: '1' }])
    // Older files without variables still open.
    const old = JSON.stringify({ version: '1', title: 'o', cells: [] })
    expect(deserializeNotebook(old).variables).toEqual([])
  })
})

describe('import', () => {
  test('markdown: sql fences become SQL cells, prose stays markdown, other code stays prose', () => {
    const cells = cellsFromMarkdown('# Report\n\nIntro\n\n```sql\nselect 1\n```\n\nMore\n\n```js\nx()\n```\n')
    expect(cells.map((c) => [c.type, c.content])).toEqual([
      ['markdown', '# Report\n\nIntro'],
      ['sql', 'select 1'],
      ['markdown', 'More\n\n```js\nx()\n```'],
    ])
  })
  test('sql script: -- %% markers, or blank-line groups', () => {
    expect(cellsFromSql('select 1;\n-- %% second\nselect 2;').map((c) => c.content)).toEqual(['select 1;', 'select 2;'])
    expect(cellsFromSql('select 1;\n\nselect 2;\nselect 3;').map((c) => c.content)).toEqual(['select 1;', 'select 2;\nselect 3;'])
  })
  test('jupyter: %%sql magics and markdown', () => {
    const ipynb = JSON.stringify({ cells: [
      { cell_type: 'markdown', source: ['# Hi\n', 'there'] },
      { cell_type: 'code', source: ['%%sql\n', 'select 1'] },
      { cell_type: 'code', source: '%sql select 2' },
      { cell_type: 'code', source: '' },
    ] })
    expect(cellsFromIpynb(ipynb).map((c) => [c.type, c.content])).toEqual([
      ['markdown', '# Hi\nthere'],
      ['sql', 'select 1'],
      ['sql', 'select 2'],
    ])
    expect(() => cellsFromIpynb('{}')).toThrow()
  })
  test('importNotebook picks the format by extension and names the notebook', () => {
    const nb = importNotebook('/x/sales report.md', '```sql\nselect 1\n```')
    expect(nb.title).toBe('sales report')
    expect(nb.cells[0].type).toBe('sql')
    expect(importNotebook('a.sql', '').cells.length).toBe(1)
  })
})

describe('export', () => {
  const nb = createNotebook('Sales')
  nb.cells = [createMarkdownCell('Some *prose*'), createSqlCell('select 1 as n')]
  nb.cells[1].result = { columns: [{ name: 'n' }], rows: [[1], [null]], queryMs: 3, error: '', message: '' }
  test('markdown with result tables', () => {
    const md = toMarkdown(nb)
    expect(md).toContain('# Sales')
    expect(md).toContain('```sql\nselect 1 as n\n```')
    expect(md).toContain('| n |\n| --- |\n| 1 |\n| NULL |')
  })
  test('sql script keeps prose as comments and cells as -- %%', () => {
    const sql = toSqlScript(nb)
    expect(sql).toContain('-- Some *prose*')
    expect(cellsFromSql(sql).map((c) => c.content).at(-1)).toBe('select 1 as n')
  })
  test('ipynb round-trips back to the same cells', () => {
    const back = cellsFromIpynb(toIpynb(nb))
    expect(back.map((c) => [c.type, c.content])).toEqual([['markdown', 'Some *prose*'], ['sql', 'select 1 as n']])
  })
  test('html escapes and includes results', () => {
    const n2 = createNotebook('<x>')
    n2.cells = [createSqlCell("select '<b>'")]
    const html = toHtml(n2, (m) => m)
    expect(html).toContain('&lt;x&gt;')
    expect(html).toContain('select &#39;&lt;b&gt;&#39;'.replace('&#39;', "'").replace('&#39;', "'"))
    expect(toHtml(nb, (m) => `<p>${m}</p>`)).toContain('<td class="null">NULL</td>')
  })
})
