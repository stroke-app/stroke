// v5: concept cut, monochrome transitions, official marks, designed SFX.
// 150 BPM grid: every scene boundary is a multiple of 0.4s.
var TL = {
  duration: 44.0,
  fps: 30,
  scenes: {
    hook: [0, 3.2],
    title: [3.2, 6.4],
    speed: [6.4, 10.4],
    engines: [10.4, 14.4],
    connect: [14.4, 19.6],
    agents: [19.6, 24.4],
    views: [24.4, 30.4],
    editing: [30.4, 36.4],
    price: [36.4, 39.6],
    outro: [39.6, 44.0],
  },
  theme: { hook: 'dark', title: 'light', speed: 'dark', engines: 'light', connect: 'dark', agents: 'light', views: 'dark', editing: 'light', price: 'dark', outro: 'dark' },
  breakT: [33.2, 36.4], // drums drop while the changes get reviewed, then land on the price
  liftT: [24.4, 39.6],
}
TL.hook = TL.scenes.hook; TL.logo = TL.scenes.title; TL.outro = TL.scenes.outro
TL.chapters = ['speed', 'engines', 'connect', 'agents', 'views', 'editing', 'price'].map(k => ({ k, t: TL.scenes[k] }))
TL.cuts = Object.values(TL.scenes).map(s => s[0]).filter(t => t > 0)
if (typeof module !== 'undefined') module.exports = TL
