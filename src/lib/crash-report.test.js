import { describe, expect, it } from 'vitest'
import { crashReportFields } from './crash-report.js'

describe('crashReportFields', () => {
  const base = { message: 'there is no reactor running', location: 'src/db/connection.rs:567', thread: 'main', version: '2.16.2', os: 'macos aarch64', at: 1763251200, backtrace: '' }

  it('titles the issue with the first line of the panic', () => {
    expect(crashReportFields({ ...base, message: 'boom\nmore' }).title).toBe('Crash: boom')
  })

  it('puts the message, place, version and platform in the details', () => {
    const d = crashReportFields(base).details
    expect(d).toContain('there is no reactor running')
    expect(d).toContain('src/db/connection.rs:567')
    expect(d).toContain('Stroke 2.16.2 on macos aarch64')
  })

  it("keeps only Stroke's own frames when the backtrace has them", () => {
    const backtrace = ['0: std::panicking::begin', '1: app_lib::db::parked::close_parked', '2: tao::event_loop::run'].join('\n')
    const d = crashReportFields({ ...base, backtrace }).details
    expect(d).toContain('app_lib::db::parked::close_parked')
    expect(d).not.toContain('tao::event_loop')
  })
})
