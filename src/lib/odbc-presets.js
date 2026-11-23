/**
 * Engines reached through the ODBC bridge (beta), and how each one's driver
 * wants its connection string.
 *
 * The keys follow each vendor's ODBC driver documentation. A preset only
 * fills the form: the string it builds is shown under the fields, and the
 * "Connection string" mode takes anything the driver accepts.
 *
 * `driver` matches the names drivers register under (odbcinst.ini, the ODBC
 * Data Source Administrator), so the form can pick the installed one.
 * `tested` marks presets run against the real database; the rest are written
 * from the vendor's reference and have not been.
 */

/**
 * @typedef {{ key: string, label: string, placeholder?: string, default?: string, optional?: boolean, file?: boolean, mono?: boolean }} OdbcField
 * @typedef {{
 *   id: string, label: string, desc: string, driver: RegExp, driverName: string,
 *   fields: OdbcField[], build: (f: Record<string, string>, driver: string) => Record<string, string | undefined>,
 *   auth?: 'userpass' | 'token' | 'none', install: string, tested?: boolean,
 * }} OdbcPreset
 */

const HOST = (placeholder = 'db.example.com') => ({ key: 'host', label: 'Host', placeholder })
const PORT = (/** @type {string} */ p) => ({ key: 'port', label: 'Port', placeholder: p, default: p })

/** @type {OdbcPreset[]} */
export const ODBC_PRESETS = [
  {
    id: 'db2',
    label: 'IBM Db2',
    desc: 'Db2 for Linux, Unix and Windows, z/OS or IBM i',
    driver: /db2|ibm data server/i,
    driverName: 'IBM DB2 ODBC DRIVER',
    fields: [HOST(), PORT('50000'), { key: 'database', label: 'Database', placeholder: 'SAMPLE' }],
    build: (f) => ({ Database: f.database, Hostname: f.host, Port: f.port, Protocol: 'TCPIP' }),
    install: 'https://www.ibm.com/support/pages/db2-odbc-cli-driver-download-and-installation-information',
  },
  {
    id: 'oracle',
    label: 'Oracle (ODBC)',
    desc: "Oracle Database through Oracle's ODBC driver",
    driver: /oracle/i,
    driverName: 'Oracle 23 ODBC driver',
    fields: [HOST(), PORT('1521'), { key: 'database', label: 'Service name', placeholder: 'FREEPDB1' }],
    build: (f) => ({ DBQ: `//${f.host}:${f.port || 1521}/${f.database}` }),
    install: 'https://www.oracle.com/database/technologies/instant-client.html',
  },
  {
    id: 'hana',
    label: 'SAP HANA',
    desc: 'HANA and HANA Cloud through the HDBODBC driver',
    driver: /hdbodbc|hana/i,
    driverName: 'HDBODBC',
    fields: [HOST('xxxx.hana.trial.ondemand.com'), PORT('30015'), { key: 'database', label: 'Tenant database', placeholder: 'HXE', optional: true }],
    // HANA Cloud listens on 443 and only speaks TLS.
    build: (f) => ({ ServerNode: `${f.host}:${f.port || 30015}`, DatabaseName: f.database, Encrypt: f.port === '443' ? 'TRUE' : undefined }),
    install: 'https://tools.hana.ondemand.com/#hanatools',
  },
  {
    id: 'ase',
    label: 'SAP ASE (Sybase)',
    desc: "Adaptive Server Enterprise, through SAP's driver or FreeTDS",
    driver: /adaptive server|sybase|\base\b|freetds/i,
    driverName: 'Adaptive Server Enterprise',
    fields: [HOST(), PORT('5000'), { key: 'database', label: 'Database', placeholder: 'master', optional: true }],
    build: (f, driver) =>
      /freetds/i.test(driver)
        ? { Server: f.host, Port: f.port, TDS_Version: '5.0', Database: f.database }
        : { Server: f.host, Port: f.port, Database: f.database },
    install: 'https://www.freetds.org/',
  },
  {
    id: 'informix',
    label: 'Informix',
    desc: 'IBM Informix through the Informix ODBC driver',
    driver: /informix/i,
    driverName: 'IBM INFORMIX ODBC DRIVER',
    fields: [
      HOST(),
      PORT('9088'),
      { key: 'server', label: 'Server name', placeholder: 'ol_informix', mono: true },
      { key: 'database', label: 'Database', placeholder: 'stores_demo' },
    ],
    build: (f) => ({ Host: f.host, Service: f.port, Server: f.server, Protocol: 'onsoctcp', Database: f.database }),
    install: 'https://www.ibm.com/products/informix/developer-tools',
  },
  {
    id: 'teradata',
    label: 'Teradata',
    desc: 'Teradata Vantage through the Teradata ODBC driver',
    driver: /teradata/i,
    driverName: 'Teradata Database ODBC Driver 20.00',
    fields: [HOST('tdprod.example.com'), { key: 'database', label: 'Default database', placeholder: 'DBC', optional: true }],
    build: (f) => ({ DBCName: f.host, Database: f.database }),
    install: 'https://downloads.teradata.com/download/connectivity/odbc-driver/linux',
  },
  {
    id: 'vertica',
    label: 'Vertica',
    desc: 'OpenText Vertica through the Vertica ODBC driver',
    driver: /vertica/i,
    driverName: 'Vertica',
    fields: [HOST(), PORT('5433'), { key: 'database', label: 'Database', placeholder: 'VMart' }],
    build: (f) => ({ Servername: f.host, Port: f.port, Database: f.database }),
    install: 'https://www.vertica.com/download/vertica/client-drivers/',
  },
  {
    id: 'exasol',
    label: 'Exasol',
    desc: 'Exasol through the EXASolution ODBC driver',
    driver: /exa/i,
    driverName: 'EXASolution Driver',
    fields: [HOST(), PORT('8563'), { key: 'fingerprint', label: 'Certificate fingerprint', placeholder: 'from the server, if self-signed', optional: true, mono: true }],
    build: (f) => ({ EXAHOST: `${f.host}:${f.port || 8563}`, FINGERPRINT: f.fingerprint }),
    install: 'https://downloads.exasol.com/clients-and-drivers/odbc',
  },
  {
    id: 'firebird',
    label: 'Firebird',
    desc: 'Firebird and InterBase through the Firebird ODBC driver',
    driver: /firebird|interbase/i,
    driverName: 'Firebird/InterBase(r) driver',
    fields: [HOST('localhost'), PORT('3050'), { key: 'database', label: 'Database file or alias', placeholder: '/var/lib/firebird/data/employee.fdb', mono: true }],
    build: (f) => ({ Dbname: `${f.host}/${f.port || 3050}:${f.database}`, CHARSET: 'UTF8' }),
    install: 'https://firebirdsql.org/en/odbc-driver/',
  },
  {
    id: 'redshift',
    label: 'Amazon Redshift',
    desc: 'Redshift clusters and Serverless through the Redshift ODBC driver',
    driver: /redshift/i,
    driverName: 'Amazon Redshift (x64)',
    fields: [HOST('my-cluster.abc123.us-east-1.redshift.amazonaws.com'), PORT('5439'), { key: 'database', label: 'Database', placeholder: 'dev', default: 'dev' }],
    build: (f) => ({ Server: f.host, Port: f.port, Database: f.database }),
    install: 'https://docs.aws.amazon.com/redshift/latest/mgmt/odbc20-install.html',
  },
  {
    id: 'bigquery',
    label: 'BigQuery (ODBC)',
    desc: "Google BigQuery through Google's ODBC driver",
    driver: /bigquery/i,
    driverName: 'Simba ODBC Driver for Google BigQuery',
    fields: [
      { key: 'project', label: 'Project ID', placeholder: 'my-project', mono: true },
      { key: 'email', label: 'Service account email', placeholder: 'reader@my-project.iam.gserviceaccount.com', mono: true },
      { key: 'keyFile', label: 'Key file', placeholder: '/path/to/key.json', file: true, mono: true },
    ],
    build: (f) => ({ Catalog: f.project, OAuthMechanism: '0', Email: f.email, KeyFilePath: f.keyFile }),
    auth: 'none',
    install: 'https://cloud.google.com/bigquery/docs/reference/odbc-jdbc-drivers',
  },
  {
    id: 'databricks',
    label: 'Databricks',
    desc: 'SQL warehouses and clusters through the Databricks ODBC driver',
    driver: /databricks|simba spark/i,
    driverName: 'Simba Spark ODBC Driver',
    fields: [
      HOST('adb-1234567890123456.7.azuredatabricks.net'),
      { key: 'httpPath', label: 'HTTP path', placeholder: '/sql/1.0/warehouses/abc123', mono: true },
    ],
    // A personal access token: user "token", the token as the password.
    build: (f) => ({ Host: f.host, Port: '443', HTTPPath: f.httpPath, SSL: '1', ThriftTransport: '2', AuthMech: '3', UID: 'token' }),
    auth: 'token',
    install: 'https://www.databricks.com/spark/odbc-drivers-download',
  },
  {
    id: 'hive',
    label: 'Apache Hive',
    desc: 'HiveServer2 through a Hive ODBC driver',
    driver: /hive/i,
    driverName: 'Cloudera ODBC Driver for Apache Hive',
    fields: [HOST(), PORT('10000'), { key: 'database', label: 'Database', placeholder: 'default', default: 'default' }],
    // AuthMech 3 is user name and password; 2 a user name alone.
    build: (f) => ({ Host: f.host, Port: f.port, Schema: f.database, HiveServerType: '2', AuthMech: f.user ? '3' : '2' }),
    install: 'https://www.cloudera.com/downloads/connectors/hive/odbc.html',
  },
  {
    id: 'trino',
    label: 'Trino and Presto',
    desc: 'Trino, Starburst or Presto through their ODBC driver',
    driver: /trino|presto|starburst/i,
    driverName: 'Starburst ODBC Driver',
    fields: [
      HOST(),
      PORT('8080'),
      { key: 'catalog', label: 'Catalog', placeholder: 'hive' },
      { key: 'schema', label: 'Schema', placeholder: 'default', optional: true },
    ],
    build: (f) => ({ Host: f.host, Port: f.port, Catalog: f.catalog, Schema: f.schema }),
    install: 'https://docs.starburst.io/clients/odbc/odbc-v2.html',
  },
  {
    id: 'dremio',
    label: 'Dremio',
    desc: 'Dremio through the Arrow Flight SQL ODBC driver',
    driver: /dremio|arrow flight/i,
    driverName: 'Arrow Flight SQL ODBC Driver',
    fields: [HOST('localhost'), PORT('32010')],
    build: (f) => ({ Host: f.host, Port: f.port, useEncryption: f.host === 'localhost' || f.host === '127.0.0.1' ? 'false' : 'true' }),
    install: 'https://docs.dremio.com/current/sonar/client-applications/drivers/arrow-flight-sql-odbc-driver/',
  },
  {
    id: 'iris',
    label: 'InterSystems IRIS',
    desc: 'IRIS and Caché through the InterSystems ODBC driver',
    driver: /iris|intersystems|cach[eé]/i,
    driverName: 'InterSystems IRIS ODBC35',
    fields: [HOST(), PORT('1972'), { key: 'database', label: 'Namespace', placeholder: 'USER', default: 'USER' }],
    build: (f) => ({ Server: f.host, Port: f.port, Database: f.database }),
    install: 'https://intersystems-community.github.io/iris-driver-distribution/',
  },
  {
    id: 'access',
    label: 'Microsoft Access',
    desc: '.mdb and .accdb files through the Access driver (Windows) or MDB Tools',
    driver: /access|mdbtools/i,
    driverName: 'Microsoft Access Driver (*.mdb, *.accdb)',
    fields: [{ key: 'file', label: 'Database file', placeholder: 'C:\\Data\\orders.accdb', file: true, mono: true }],
    build: (f) => ({ Dbq: f.file }),
    auth: 'none',
    install: 'https://www.microsoft.com/en-us/download/details.aspx?id=54920',
  },
]

/** The generic bridge: a DSN or any connection string. */
export const GENERIC_ODBC = {
  id: 'generic',
  label: 'ODBC',
  desc: 'Any database with an ODBC driver: a DSN or a connection string',
}

/** @param {string | undefined | null} id */
export function odbcPreset(id) {
  return ODBC_PRESETS.find((p) => p.id === id) ?? null
}

/** The picker id for a preset: `odbc-db2`, or `odbc` for the generic bridge. */
export function odbcDriverId(/** @type {string | undefined | null} */ preset) {
  return preset && preset !== 'generic' && odbcPreset(preset) ? `odbc-${preset}` : 'odbc'
}

/** The preset behind a picker id (`odbc-db2` → `db2`), `generic` for `odbc`. */
export function presetOfDriverId(/** @type {string} */ id) {
  return id.startsWith('odbc-') ? id.slice(5) : 'generic'
}

/** An ODBC connection string value: braced when it holds `;`, `{`, `}` or `=`. */
export function odbcValue(/** @type {string} */ v) {
  return /[;{}=]|^\s|\s$/.test(v) ? `{${v.replace(/}/g, '}}')}}` : v
}

/**
 * The installed driver a preset should use: the first whose name matches.
 * @param {OdbcPreset} preset @param {string[]} installed
 */
export function pickDriver(preset, installed) {
  return installed.find((d) => preset.driver.test(d)) ?? ''
}

/**
 * The connection string for a preset's fields (no credentials: those are
 * added by the backend from the user and password fields).
 * @param {OdbcPreset} preset @param {string} driver @param {Record<string, string>} fields
 */
export function presetConnectionString(preset, driver, fields) {
  const f = Object.fromEntries(Object.entries(fields).map(([k, v]) => [k, String(v ?? '').trim()]))
  const parts = [`Driver={${(driver || preset.driverName).replace(/}/g, '}}')}}`]
  for (const [k, v] of Object.entries(preset.build(f, driver || preset.driverName))) {
    if (v !== undefined && v !== '') parts.push(`${k}=${odbcValue(v)}`)
  }
  return parts.join(';') + ';'
}

/**
 * What a saved ODBC connection hands the backend.
 * @param {any} c a saved connection (type 'odbc')
 */
export function odbcConfigOf(c) {
  const preset = odbcPreset(c.preset)
  const mode = c.odbcMode || (preset ? 'fields' : c.dsn ? 'dsn' : 'string')
  const base = { name: String(c.name || 'ODBC'), preset: c.preset || 'generic', user: c.user || null, password: c.password || null }
  if (mode === 'dsn') return { ...base, dsn: String(c.dsn || '').trim(), connectionString: null }
  if (mode === 'fields' && preset) {
    const fields = { host: c.host ?? '', port: c.port != null ? String(c.port) : '', database: c.database ?? '', user: c.user ?? '', ...(c.odbcFields ?? {}) }
    const user = preset.auth === 'token' ? null : base.user
    // Through SSH: the backend opens the tunnel to this host and port and
    // points the string at its local end.
    const port = Number(c.port)
    const ssh = c.ssh && fields.host && port ? { ssh: c.ssh, targetHost: fields.host, targetPort: port } : {}
    return { ...base, user, dsn: null, connectionString: presetConnectionString(preset, c.odbcDriver || '', fields), ...ssh }
  }
  return { ...base, dsn: null, connectionString: String(c.connectionString || '').trim() }
}
