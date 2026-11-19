/**
 * A crash report left by the last run (src-tauri/src/crash.rs), as the fields
 * of a GitHub issue. Only what the panic hook recorded goes in: the message,
 * where it happened, the version and the platform. The backtrace is trimmed to
 * Stroke's own frames when there are any, and capped, since a URL carries it.
 *
 * @param {{ message: string, location: string, thread: string, version: string, os: string, at: number, backtrace: string }} c
 */
export function crashReportFields(c) {
  const when = c.at ? new Date(c.at * 1000).toISOString().replace('T', ' ').slice(0, 19) + ' UTC' : 'unknown'
  const frames = String(c.backtrace || '').split('\n')
  const ours = frames.filter((l) => /\bapp(_lib)?::|src[\\/]/.test(l))
  const trace = (ours.length ? ours : frames).join('\n').slice(0, 2500)
  const short = String(c.message || 'panic').split('\n')[0].slice(0, 90)
  return {
    title: `Crash: ${short}`,
    body: 'Stroke closed by itself. What I was doing just before:\n\n',
    details: [
      `${c.message}`,
      `at ${c.location || 'unknown location'} (thread ${c.thread || 'unnamed'})`,
      `Stroke ${c.version} on ${c.os}, ${when}`,
      trace ? `\n${trace}` : '',
    ].join('\n').trim(),
  }
}
