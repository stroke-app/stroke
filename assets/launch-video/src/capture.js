// Renders index.html frame by frame in headless Chromium.
//   node capture.js cues              -> cues.json (sound cue list the page exports)
//   node capture.js stills 3.5 12 ...  -> stills/t_<time>.png
//   node capture.js video             -> video.mp4 (silent, frames piped to ffmpeg)
// Set CHROME to a Chromium binary if Playwright's bundled one is not installed.
const { chromium } = require('playwright-core')
const { spawn } = require('child_process')
const fs = require('fs')
const path = require('path')
const TL = require('./timeline.js')

function chromePath() {
  if (process.env.CHROME) return process.env.CHROME
  const root = path.join(process.env.HOME || '', '.cache/ms-playwright')
  if (fs.existsSync(root)) {
    const dir = fs.readdirSync(root).filter(d => /^chromium-\d+$/.test(d)).sort().pop()
    const bin = dir && path.join(root, dir, 'chrome-linux64/chrome')
    if (bin && fs.existsSync(bin)) return bin
  }
  return '/usr/bin/chromium'
}

async function main() {
  const [mode, ...times] = process.argv.slice(2)
  const browser = await chromium.launch({ executablePath: chromePath(), args: ['--allow-file-access-from-files', '--force-color-profile=srgb'] })
  const page = await browser.newPage({ viewport: { width: 1920, height: 1080 }, deviceScaleFactor: 1 })
  await page.goto('file://' + path.join(__dirname, 'index.html'))
  await page.evaluate(() => window.ready)
  if (mode === 'cues') {
    const cues = await page.evaluate(() => window.CUES)
    fs.writeFileSync(path.join(__dirname, 'cues.json'), JSON.stringify(cues))
    console.log('cues', cues.length)
  } else if (mode === 'stills') {
    fs.mkdirSync(path.join(__dirname, 'stills'), { recursive: true })
    for (const t of times) {
      await page.evaluate(t => window.render(t), Number(t))
      await page.screenshot({ path: path.join(__dirname, 'stills', 't_' + t + '.png') })
    }
  } else if (mode === 'video') {
    const total = Math.round(TL.duration * TL.fps)
    const ff = spawn('ffmpeg', ['-y', '-loglevel', 'error', '-f', 'image2pipe', '-framerate', String(TL.fps), '-c:v', 'png', '-i', '-',
      '-c:v', 'libx264', '-preset', 'slow', '-crf', '16', '-pix_fmt', 'yuv420p', '-movflags', '+faststart', path.join(__dirname, 'video.mp4')], { stdio: ['pipe', 'inherit', 'inherit'] })
    for (let i = 0; i < total; i++) {
      await page.evaluate(t => window.render(t), i / TL.fps)
      const buf = await page.screenshot({ type: 'png' })
      if (!ff.stdin.write(buf)) await new Promise(r => ff.stdin.once('drain', r))
      if (i % 120 === 0) process.stdout.write('frame ' + i + '/' + total + '\n')
    }
    ff.stdin.end()
    await new Promise(r => ff.on('close', r))
  } else {
    console.error('usage: node capture.js cues | stills <t...> | video')
    process.exitCode = 1
  }
  await browser.close()
}
main().catch(e => { console.error(e); process.exit(1) })
