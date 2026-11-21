/**
 * How long an error toast stays: long enough to read it. The default gives
 * every toast 4.5 s, which suits "Copied" and loses a four-line driver error
 * before it is read. 4.5 s plus 40 ms a character, at most 12 s; hovering
 * still holds it.
 * @param {unknown} title @param {any} [opts]
 */
export function errorDuration(title, opts) {
  const text = `${typeof title === 'string' ? title : ''} ${typeof opts?.description === 'string' ? opts.description : ''}`.trim()
  return Math.min(12_000, 4_500 + text.length * 40)
}
