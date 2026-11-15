// Every `<Component>` a .svelte file renders must be in scope where it's
// rendered. Svelte compiles an undeclared one without complaint and the page
// then throws "Can't find variable: Icon" when it renders; svelte-check
// doesn't report it for JavaScript components. A nightly shipped a welcome
// screen that crashed this way.
import { describe, expect, it } from 'vitest'
import { readFileSync, readdirSync } from 'node:fs'
import { join } from 'node:path'
import { parse } from 'svelte/compiler'

/** @param {string} dir @returns {string[]} */
function svelteFiles(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? svelteFiles(join(dir, e.name)) : e.name.endsWith('.svelte') ? [join(dir, e.name)] : [],
  )
}

/** Names bound by a destructuring pattern. @param {any} p @returns {string[]} */
function patternNames(p) {
  if (!p) return []
  switch (p.type) {
    case 'Identifier':
      return [p.name]
    case 'ObjectPattern':
      return p.properties.flatMap((/** @type {any} */ q) => patternNames(q.type === 'RestElement' ? q.argument : q.value))
    case 'ArrayPattern':
      return p.elements.flatMap(patternNames)
    case 'AssignmentPattern':
      return patternNames(p.left)
    case 'RestElement':
      return patternNames(p.argument)
    default:
      return []
  }
}

/** Top-level names a `<script>` declares. @param {any} script */
function scriptNames(script) {
  if (!script) return []
  return script.content.body.flatMap((/** @type {any} */ s) => {
    const d = s.type === 'ExportNamedDeclaration' && s.declaration ? s.declaration : s
    if (d.type === 'ImportDeclaration') return d.specifiers.map((/** @type {any} */ x) => x.local.name)
    if (d.type === 'VariableDeclaration') return d.declarations.flatMap((/** @type {any} */ x) => patternNames(x.id))
    if ((d.type === 'FunctionDeclaration' || d.type === 'ClassDeclaration') && d.id) return [d.id.name]
    return []
  })
}

/** `<Name>` tags rendered where `Name` isn't in scope. @param {any} ast */
function undeclared(ast) {
  /** @type {string[]} */
  const missing = []
  const top = new Set([...scriptNames(ast.instance), ...scriptNames(ast.module)])

  /** @param {any} node @param {Set<string>} scope */
  const visit = (node, scope) => {
    if (!node || typeof node !== 'object') return
    if (Array.isArray(node)) return node.forEach((n) => visit(n, scope))
    if (node.type === 'Fragment') {
      // {@const} declares for its whole fragment.
      const inner = new Set(scope)
      for (const n of node.nodes) if (n.type === 'ConstTag') for (const x of n.declaration.declarations) patternNames(x.id).forEach((v) => inner.add(v))
      return node.nodes.forEach((/** @type {any} */ n) => visit(n, inner))
    }
    let s = scope
    if (node.type === 'SnippetBlock') {
      s = new Set(scope)
      node.parameters.flatMap(patternNames).forEach((/** @type {string} */ v) => s.add(v))
    } else if (node.type === 'EachBlock') {
      s = new Set(scope)
      patternNames(node.context).forEach((v) => s.add(v))
      if (node.index) s.add(node.index)
    } else if (node.type === 'AwaitBlock') {
      // `then`/`catch` bindings reach only their own branch.
      visit(node.pending, scope)
      visit(node.then, new Set([...scope, ...patternNames(node.value)]))
      visit(node.catch, new Set([...scope, ...patternNames(node.error)]))
      return
    }
    if (node.type === 'Component' && /^[A-Z]\w*$/.test(node.name) && !s.has(node.name)) missing.push(node.name)
    for (const [k, v] of Object.entries(node)) {
      if (k === 'parent' || k === 'metadata' || !v || typeof v !== 'object') continue
      visit(v, s)
    }
  }
  visit(ast.fragment, top)
  return [...new Set(missing)]
}

describe('rendered components are in scope', () => {
  const files = svelteFiles(join(process.cwd(), 'src'))
  it('finds the app components', () => expect(files.length).toBeGreaterThan(50))

  it('catches a component used without an import', () => {
    const src = `<script>import A from './A.svelte'</script>\n{#snippet row(Icon)}<Icon />{/snippet}\n<A /><Icon name="x" />`
    expect(undeclared(parse(src, { modern: true }))).toEqual(['Icon'])
  })

  for (const f of files) {
    it(f.slice(process.cwd().length + 1), () => {
      expect(undeclared(parse(readFileSync(f, 'utf8'), { modern: true }))).toEqual([])
    })
  }
})
