import { describe, expect, it } from 'vitest'
import {
  ODBC_PRESETS,
  odbcConfigOf,
  odbcDriverId,
  odbcPreset,
  odbcValue,
  pickDriver,
  presetConnectionString,
  presetOfDriverId,
} from './odbc-presets.js'

describe('ODBC presets', () => {
  it('have unique ids and a driver name each', () => {
    const ids = ODBC_PRESETS.map((p) => p.id)
    expect(new Set(ids).size).toBe(ids.length)
    for (const p of ODBC_PRESETS) {
      expect(p.driverName).toBeTruthy()
      expect(p.install).toMatch(/^https:\/\//)
      // Each preset's own driver name is one its pattern recognises.
      expect(p.driver.test(p.driverName)).toBe(true)
    }
  })

  it('map picker ids both ways', () => {
    expect(odbcDriverId('db2')).toBe('odbc-db2')
    expect(odbcDriverId('generic')).toBe('odbc')
    expect(odbcDriverId('nope')).toBe('odbc')
    expect(presetOfDriverId('odbc-hana')).toBe('hana')
    expect(presetOfDriverId('odbc')).toBe('generic')
  })

  it('brace values the connection string syntax would split', () => {
    expect(odbcValue('plain')).toBe('plain')
    expect(odbcValue('a;b')).toBe('{a;b}')
    expect(odbcValue('x}y')).toBe('{x}}y}')
    expect(odbcValue(' padded')).toBe('{ padded}')
  })

  it('pick the installed driver that matches', () => {
    const installed = ['PostgreSQL Unicode', 'IBM DB2 ODBC DRIVER - DB2COPY1', 'HDBODBC']
    expect(pickDriver(odbcPreset('db2'), installed)).toBe('IBM DB2 ODBC DRIVER - DB2COPY1')
    expect(pickDriver(odbcPreset('hana'), installed)).toBe('HDBODBC')
    expect(pickDriver(odbcPreset('teradata'), installed)).toBe('')
  })

  it('build each engine its own string', () => {
    const db2 = odbcPreset('db2')
    expect(presetConnectionString(db2, '', { host: 'h', port: '50000', database: 'SAMPLE' })).toBe(
      'Driver={IBM DB2 ODBC DRIVER};Database=SAMPLE;Hostname=h;Port=50000;Protocol=TCPIP;',
    )
    expect(presetConnectionString(odbcPreset('hana'), 'HDBODBC', { host: 'x.hana.ondemand.com', port: '443', database: '' })).toBe(
      'Driver={HDBODBC};ServerNode=x.hana.ondemand.com:443;Encrypt=TRUE;',
    )
    expect(presetConnectionString(odbcPreset('ase'), 'FreeTDS', { host: 'h', port: '5000', database: 'pubs2' })).toContain('TDS_Version=5.0')
    expect(presetConnectionString(odbcPreset('ase'), 'Adaptive Server Enterprise', { host: 'h', port: '5000' })).not.toContain('TDS_Version')
    expect(presetConnectionString(odbcPreset('oracle'), 'Oracle 23 ODBC driver', { host: 'h', port: '1521', database: 'FREEPDB1' })).toBe(
      'Driver={Oracle 23 ODBC driver};DBQ=//h:1521/FREEPDB1;',
    )
    expect(presetConnectionString(odbcPreset('firebird'), '', { host: 'h', port: '3050', database: '/data/e.fdb' })).toContain(
      'Dbname=h/3050:/data/e.fdb;CHARSET=UTF8;',
    )
    expect(presetConnectionString(odbcPreset('databricks'), '', { host: 'adb.net', httpPath: '/sql/1.0/warehouses/x' })).toContain(
      'HTTPPath=/sql/1.0/warehouses/x;SSL=1;ThriftTransport=2;AuthMech=3;UID=token;',
    )
    // Empty optional fields are left out, not written as `Key=;`.
    expect(presetConnectionString(odbcPreset('trino'), 'Trino', { host: 'h', port: '8080', catalog: 'hive', schema: '' })).toBe(
      'Driver={Trino};Host=h;Port=8080;Catalog=hive;',
    )
  })

  it('turn a saved connection into what the backend takes', () => {
    expect(
      odbcConfigOf({ name: 'DW', type: 'odbc', preset: 'vertica', odbcDriver: 'Vertica', host: 'v', port: 5433, database: 'VMart', user: 'dbadmin', password: 'pw' }),
    ).toEqual({ name: 'DW', preset: 'vertica', user: 'dbadmin', password: 'pw', dsn: null, connectionString: 'Driver={Vertica};Servername=v;Port=5433;Database=VMart;' })
    expect(odbcConfigOf({ type: 'odbc', preset: 'generic', odbcMode: 'dsn', dsn: ' Sales ' })).toMatchObject({ dsn: 'Sales', connectionString: null })
    expect(odbcConfigOf({ type: 'odbc', odbcMode: 'string', connectionString: 'Driver={X};A=1;' })).toMatchObject({ dsn: null, connectionString: 'Driver={X};A=1;' })
    // Databricks: the token is the password, the user is "token" in the string.
    const dbx = odbcConfigOf({ type: 'odbc', preset: 'databricks', host: 'h', odbcFields: { httpPath: '/p' }, user: 'me', password: 'dapi1' })
    expect(dbx.user).toBeNull()
    expect(dbx.password).toBe('dapi1')
  })
})
