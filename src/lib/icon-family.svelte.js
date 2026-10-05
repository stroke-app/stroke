/**
 * The active icon family and weight, subscribed once for the whole app.
 *
 * Icon reads two settings stores, and reading a store inside a component
 * subscribes that component. A sidebar listing a few hundred tables draws two
 * or three icons a row, so first paint was standing up several hundred
 * subscriptions and their teardown for a pair of values that are the same for
 * every icon on screen and change only when the setting does.
 *
 * Module scope, so there are two subscriptions in the process however many
 * icons are mounted. They are never torn down on purpose: both stores live as
 * long as the app does.
 */
import { appIconSet, appIconStyle } from '$lib/stores/settings.js'

let set = $state('hugeicons')
let style = $state('regular')
/** The Phosphor components, once loaded: they are not in the startup bundle. @type {Record<string, any> | null} */
let phosphorMap = $state(null)
/** @type {Promise<void> | null} */
let phosphorLoad = null

function loadPhosphor() {
  phosphorLoad ??= import('$lib/icon-registry-phosphor.js')
    .then((m) => { phosphorMap = m.PHOSPHOR_MAP })
    .catch(() => { phosphorLoad = null })
}

appIconSet.subscribe((v) => {
  set = v
  if (v === 'phosphor') loadPhosphor()
})
appIconStyle.subscribe((v) => { style = v })

export const iconFamily = {
  get set() { return set },
  get style() { return style },
  /** Phosphor's components by icon name, or null until the set has loaded. */
  get phosphor() { return phosphorMap },
  /** Phosphor carries weight in the glyph rather than a stroke width. */
  get phosphorWeight() {
    return style === 'light' ? 'light' : style === 'bold' ? 'bold' : 'regular'
  },
}
