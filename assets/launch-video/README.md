# Launch video

[![Stroke launch video](stroke-launch.gif)](https://media.stroke.click/launch/stroke-launch-1080p60.mp4?v=1)

Hosted on Cloudflare R2 (the `stroke` bucket, served at media.stroke.click): [1080p60](https://media.stroke.click/launch/stroke-launch-1080p60.mp4?v=1) for the web, [4K 60fps](https://media.stroke.click/launch/stroke-launch-2160p60.mp4?v=1) as the master. The landing page plays the 1080p60 cut.

`stroke-launch.mp4` is 44 seconds, 1920×1080 at 30fps with stereo AAC. `stroke-launch.jpg` is its poster (the final frame, also baked into frame 0 so thumbnails show it). `stroke-launch.gif` is a silent 800px loop of the whole video for the README, because GitHub only plays videos uploaded as attachments, never ones stored in the repo.

## What it shows

| Time | Scene |
|---|---|
| 0-3s | Humans. Agents. Both need a database. |
| 3-6s | The database studio for agents and humans. |
| 6-10s | A million rows paging past, and a fan that never spins |
| 10-14s | Ten engines, from a local SQLite file to ClickHouse |
| 14-20s | Sign in to one of 11 providers, or open a Docker database Stroke found on its own |
| 20-24s | People and agents on one database: the in-app AI chat, Claude, Cursor and any MCP client |
| 24-30s | One query result read six ways: table, JSON, text, chart, ERD, map |
| 30-36s | Staged edits: enum picker, right-click delete, a new row, SQL review, apply |
| 36-40s | $9.99 once, against $100+ a year |
| 40-44s | Stroke, stroke.click |

## How it's built

There is no video editor and no screen recording. Every frame is drawn by a web page.

- `src/index.html` draws the whole video as a pure function of time: `window.render(t)` sets every element for second `t`. The Stroke cube is a vector rebuild of `public/stroke_white.png`, the engine and provider logos come from `src/lib/db-icons.js`, the Claude, Cursor, MCP, Apple and Linux marks from simple-icons, the UI icons from Lucide, and the type is Geist.
- `src/timeline.js` holds the scene times on a 150 BPM grid, so every cut lands on a beat.
- `src/capture.js` loads the page in headless Chromium, renders each of the 1,320 frames and pipes them to ffmpeg.
- The page also exports its sound cues (`window.CUES`, 249 of them: clicks, keys, pops, whooshes, impacts, scribbles, chimes). `src/audio.js` synthesizes the music bed and every effect from scratch, with no samples, and ducks the music under the effects.
- `src/build.sh` runs all of it, masters the audio to -14 LUFS and muxes the result.

## Rebuild it

Needs Node 20+, ffmpeg and a Chromium. Playwright's bundled one is found automatically; otherwise set `CHROME=/path/to/chromium`.

```bash
cd assets/launch-video/src
bash build.sh
```

A full render takes about five minutes and writes `stroke-launch.mp4`, `stroke-launch.jpg` and `stroke-launch.gif` one level up.

For the 4K 60fps master, render with `DSF=2 FPS=60 CRF=12 OUT=video_2160p60.mp4 node capture.js video` and mux the audio onto it. The page is vector, so it stays sharp at any size.

To change it:

- Scene timing lives in `timeline.js`, the scenes in `index.html`, the sound in `audio.js`.
- `npm run stills -- 16.5 30.2` renders single frames to `stills/` for a quick check.
- `npm run marks` rebuilds `marks.js` after the logos in `src/lib/db-icons.js` change.

All copy comes from stroke.click and the main README. The database names, countries, plan rows and Docker containers in the connect, views and editing scenes are illustrative.

## Credits

- Logos: [simple-icons](https://simpleicons.org) (CC0) and the marks in `src/lib/db-icons.js`. Brand names and logos belong to their owners.
- Icons: [Lucide](https://lucide.dev) (ISC).
- Type: [Geist](https://vercel.com/font) (OFL).
- The video's structure follows the brag-slim recipe from [latent-spaces/brag](https://github.com/latent-spaces/brag).
