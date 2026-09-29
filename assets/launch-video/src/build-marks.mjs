// Rebuilds marks.js: the official logos the video draws.
// Engines, providers and Docker come from the app's own src/lib/db-icons.js,
// OpenAI from src/lib/brand-icons.js, and the agent + OS marks from simple-icons
// (CC0, installed as a dev dependency here). Windows is its four-pane mark.
import fs from 'fs'
import path from 'path'
import { fileURLToPath } from 'url'

const here = path.dirname(fileURLToPath(import.meta.url))
const lib = path.join(here, '../../../src/lib')
const { BRAND_MARKS } = await import(path.join(lib, 'db-icons.js'))
const { BRAND_PATHS } = await import(path.join(lib, 'brand-icons.js'))
const SI = path.join(here, 'node_modules/simple-icons/icons')

const out = {}
for (const [k, v] of Object.entries(BRAND_MARKS)) out[k] = { d: v.d, vb: v.vb || '0 0 24 24', hex: v.hex }
const si = { claude: 'D97757', cursor: '000000', githubcopilot: '000000', windsurf: '0B100F', apple: '000000', linux: 'FCC624', modelcontextprotocol: '000000' }
for (const [k, hex] of Object.entries(si)) {
  const svg = fs.readFileSync(path.join(SI, k + '.svg'), 'utf8')
  out[k] = { d: svg.match(/ d="([^"]+)"/)[1], vb: svg.match(/viewBox="([^"]+)"/)[1], hex: '#' + hex }
}
out.openai = { d: BRAND_PATHS.openai, vb: '0 0 24 24', hex: '#000000' }
out.windows = { d: 'M0 0h11.4v11.4H0zM12.6 0H24v11.4H12.6zM0 12.6h11.4V24H0zM12.6 12.6H24V24H12.6z', vb: '0 0 24 24', hex: '#0078D4' }
fs.writeFileSync(path.join(here, 'marks.js'), 'window.MARKS=' + JSON.stringify(out))
console.log('marks.js:', Object.keys(out).length, 'marks')
