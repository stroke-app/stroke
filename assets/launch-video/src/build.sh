#!/usr/bin/env bash
# Renders ../stroke-launch.mp4 and ../stroke-launch.jpg from index.html.
# Needs Node 20+, ffmpeg, and a Chromium (Playwright's, or set CHROME=/path/to/chromium).
set -euo pipefail
cd "$(dirname "$0")"
ROOT=../../..

# fonts come from the app's own dependencies
cp "$ROOT/node_modules/@fontsource-variable/geist/files/geist-latin-wght-normal.woff2" geist.woff2
cp "$ROOT/node_modules/@fontsource-variable/geist-mono/files/geist-mono-latin-wght-normal.woff2" geist-mono.woff2
[ -d node_modules ] || npm install --no-audit --no-fund

node capture.js cues   # sound cue list from the page
node audio.js          # music + SFX -> music.wav
ffmpeg -loglevel error -y -i music.wav \
  -af "lowpass=f=14000,equalizer=f=3500:t=q:w=1:g=-1.5,acompressor=threshold=-18dB:ratio=2.5:attack=8:release=120,loudnorm=I=-14:TP=-1.5:LRA=8,aresample=48000" \
  -c:a pcm_s16le music_master.wav
node capture.js video  # frames -> video.mp4

# poster: the settled final frame, also baked into frame 0 so thumbnails show it
node capture.js stills 43.2
ffmpeg -loglevel error -y -i stills/t_43.2.png -q:v 2 ../stroke-launch.jpg
ffmpeg -loglevel error -y -i video.mp4 -loop 1 -i ../stroke-launch.jpg -i music_master.wav \
  -filter_complex "[0:v][1:v]overlay=enable='eq(n\,0)':shortest=1,format=yuv420p[v]" \
  -map "[v]" -map 2:a -c:v libx264 -preset slow -crf 17 -c:a aac -b:a 192k -shortest -movflags +faststart ../stroke-launch.mp4
# silent looping preview for the README (GitHub will not play repo videos inline)
ffmpeg -loglevel error -y -i ../stroke-launch.mp4 -vf "fps=12,scale=800:-2:flags=lanczos,palettegen=max_colors=96:stats_mode=diff" stills/palette.png
ffmpeg -loglevel error -y -i ../stroke-launch.mp4 -i stills/palette.png \
  -lavfi "fps=12,scale=800:-2:flags=lanczos[x];[x][1:v]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle" ../stroke-launch.gif
echo "wrote assets/launch-video/stroke-launch.mp4 and stroke-launch.gif"
