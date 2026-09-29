<script>
  import { tick, untrack, onDestroy } from "svelte";
  import Icon from "./Icon.svelte";
  import CloudflareLogin from "./CloudflareLogin.svelte";
  import ProviderConnect from "./ProviderConnect.svelte";
  import DbIcon from "./DbIcon.svelte";
  import {
    testPostgresConnection,
    connectPostgres,
    testSqliteConnection,
    connectSqlite,
    testMysqlConnection,
    connectMysql,
    testD1Connection,
    connectD1,
    testLibSqlConnection,
    connectLibSql,
    testClickhouseConnection,
    connectClickhouse,
    testDuckdbConnection,
    connectDuckdb,
    testMssqlConnection,
    connectMssql,
    testRedis,
    connectRedis,
    scanLocalStudios,
    scanDockerDatabases,
    scanMachineDatabases,
  } from "$lib/api.js";
  import {
    loadSavedConnections,
    upsertConnection,
    lastPersistFailed,
    removeConnection,
    newConnectionId,
    getLastConnectionId,
    setLastConnectionId,
    findDuplicateConnection,
  } from "$lib/stores/connections.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import SearchableMenu from "./SearchableMenu.svelte";
  import { Popover, PopoverTrigger, PopoverContent } from "$lib/components/ui/popover/index.js";
  import PasswordInput from "./PasswordInput.svelte";
  import Kbd from "./Kbd.svelte";
  import { requireUnlock } from "$lib/stores/app-lock.js";
  import { readClipboardText } from "$lib/clipboard.js";
  import { Checkbox } from "$lib/components/ui/checkbox/index.js";
  import { ScrollArea } from "$lib/components/ui/scroll-area/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Dialog as DialogPrimitive } from "bits-ui";
  import ResizeHandle from "./ResizeHandle.svelte";
  import { cn } from "$lib/utils.js";
  import { IS_MAC } from '$lib/shortcuts.js';
  import { focusTrap } from '$lib/actions/focus-trap.js';
  import { toast } from "$lib/components/ui/sonner/toast.svelte.js";
  import { parseConnectionUri, detectConnectionUri } from "$lib/connection-uri.js";
  import { PROVIDERS, providerBuildConnection } from "$lib/providers.js";
  import { providerOf, engineLabel } from "$lib/connection-provider.js";
  import ConfirmDialog from "./ConfirmDialog.svelte";

  let {
    open = $bindable(false),
    /**
     * Engine to open straight into, e.g. from the welcome screen's chips. Cleared
     * once consumed so the next plain open starts on the picker again.
     */
    initialEngine = $bindable(""),
    onconnected = (conn, id) => {},
    maxConnections = Infinity,
    /** Name of the live session, '' when nothing is connected. Drives Disconnect. */
    activeConnectionName = "",
    ondisconnect = () => {},
    /** A saved connection was deleted here; the shell ends its session if open. */
    onremoved = (/** @type {string} */ id) => {},
  } = $props();

  const CATEGORIES = [
    {
      label: "Relational",
      drivers: [
        {
          id: "postgres",
          label: "PostgreSQL",
          desc: "Open-source relational database",
        },
        { id: "mysql", label: "MySQL", desc: "Popular relational database" },
        {
          id: "mariadb",
          label: "MariaDB",
          desc: "MySQL-compatible relational database",
        },
        {
          id: "cockroachdb",
          label: "CockroachDB",
          desc: "Distributed Postgres-compatible SQL",
        },
        { id: "mssql", label: "SQL Server", desc: "Microsoft SQL Server" },
      ],
    },
    {
      label: "SQLite",
      drivers: [
        { id: "sqlite", label: "SQLite", desc: "Local file-based database" },
        {
          id: "sqlite-memory",
          label: "In-Memory",
          desc: "Ephemeral, nothing on disk",
        },
        {
          id: "libsql",
          label: "Turso / LibSQL",
          desc: "Serverless SQLite at the edge",
        },
      ],
    },
    {
      label: "Analytics",
      drivers: [
        {
          id: "clickhouse",
          label: "ClickHouse",
          desc: "Columnar OLAP over HTTP",
        },
        {
          id: "duckdb",
          label: "DuckDB",
          desc: "In-process analytical database",
        },
        {
          id: "duckdb-memory",
          label: "DuckDB In-Memory",
          desc: "Ephemeral DuckDB, nothing on disk",
        },
      ],
    },
    {
      label: "Cloud",
      drivers: [
        { id: "d1", label: "Cloudflare D1", desc: "Edge SQLite via REST API" },
        { id: "redis", label: "Redis", desc: "In-memory key-value store" },
      ],
    },
    {
      label: "Hosting providers",
      drivers: [
        {
          id: "neon",
          label: "Neon",
          desc: "Serverless Postgres, sign in & pick a database",
        },
        {
          id: "supabase",
          label: "Supabase",
          desc: "Postgres platform, sign in & pick a project",
        },
        {
          id: "planetscale",
          label: "PlanetScale",
          desc: "Serverless MySQL, sign in & pick a database",
        },
        {
          id: "prisma",
          label: "Prisma Postgres",
          desc: "Paste a Prisma Postgres connection string",
        },
        {
          id: "tidb",
          label: "TiDB Cloud",
          desc: "Serverless MySQL, sign in & pick a cluster",
        },
        {
          id: "turso",
          label: "Turso",
          desc: "Edge SQLite, sign in & pick a database",
        },
        {
          id: "railway",
          label: "Railway",
          desc: "Postgres, MySQL & Redis, sign in & pick a service",
        },
        {
          id: "nile",
          label: "Nile",
          desc: "Multi-tenant Postgres, sign in & pick a database",
        },
        {
          id: "upstash",
          label: "Upstash",
          desc: "Serverless Redis, connect with an API key",
        },
      ],
    },
  ];

  const ALL_DRIVERS = CATEGORIES.flatMap((c) => c.drivers);
  function driverById(id) {
    return ALL_DRIVERS.find((d) => d.id === id) ?? ALL_DRIVERS[0];
  }

  // Flat, explicitly-ordered list for the searchable Type dropdown.
  // PostgreSQL → SQLite → MySQL pinned to the top, then the rest.
  const DRIVER_ORDER = [
    "postgres",
    "sqlite",
    "mysql",
    "mariadb",
    "cockroachdb",
    "mssql",
    "clickhouse",
    "duckdb",
    "sqlite-memory",
    "duckdb-memory",
    "libsql",
    "neon",
    "supabase",
    "planetscale",
    "prisma",
    "tidb",
    "turso",
    "railway",
    "nile",
    "upstash",
    "d1",
    "redis",
  ];
  const driverItems = DRIVER_ORDER.map((id) =>
    ALL_DRIVERS.find((d) => d.id === id),
  )
    .filter(Boolean)
    .map((d) => ({
      value: d.id,
      label: d.label,
      keywords: [d.label, d.desc],
      disabled: !!d.soon,
    }));

  // Provider (sign-in) ids are surfaced as cards on their own tab, so keep them
  // out of the manual Type dropdown.
  const PROVIDER_IDS = ["neon", "supabase", "planetscale", "prisma", "tidb", "turso", "railway", "nile", "upstash"];
  // Providers temporarily turned off (shown as a disabled tab, not connectable).
  // Railway: the adapter is done, but its OAuth app isn't registered yet, so
  // there is no client id to sign in with.
  /** @type {Set<string>} */
  const DISABLED_TABS = new Set(["railway"]);

  // Subtle per-engine icon tint (color-500/600), theme-aware via Tailwind tokens.
  const ENGINE_TINT = {
    postgres: "text-sky-500/80",
    cockroachdb: "text-teal-500/80",
    mysql: "text-cyan-500/80", // the dolphin is teal, not amber
    mariadb: "text-orange-500/80",
    sqlite: "text-blue-500/80",
    "sqlite-memory": "text-blue-500/80",
    mssql: "text-red-500/80",
    clickhouse: "text-yellow-500/80",
    duckdb: "text-yellow-500/80",
    "duckdb-memory": "text-yellow-500/80",
    d1: "text-orange-500/80",
    libsql: "text-emerald-500/80",
    docker: "text-sky-500/80",
    neon: "text-emerald-500/80",
    supabase: "text-emerald-500/80",
    planetscale: "text-foreground/70",
    prisma: "text-indigo-500/80",
    tidb: "text-red-500/80",
    turso: "text-teal-500/80",
    railway: "text-foreground/80",
    nile: "text-violet-500/80",
    upstash: "text-emerald-500/80",
    drizzle: "text-lime-500/80",
    redis: "text-red-500/80",
  };
  function engineTint(id) {
    return ENGINE_TINT[id] ?? "text-muted-foreground";
  }

  /** Most recently connected first; never-connected rows sink to the bottom. */
  const byLastConnected = (a, b) =>
    (b.lastConnectedAt ?? 0) - (a.lastConnectedAt ?? 0);

  let saved = $state(loadSavedConnections().sort(byLastConnected));
  /** Filter over the saved rail. Matches the things you would recognise a
   *  connection by - its name, its engine, and where it points. */
  let savedQuery = $state("");
  /** @type {HTMLInputElement | null} */
  let savedSearchEl = $state(null);
  const savedMatches = $derived.by(() => {
    const q = savedQuery.trim().toLowerCase();
    if (!q) return saved;
    return saved.filter((c) =>
      [c.name, c.type, c.database, c.host, c.filePath, c.libsqlUrl]
        .filter(Boolean)
        .some((v) => String(v).toLowerCase().includes(q)),
    );
  });
  /** What Enter in the filter acts on - same target the eye is already on. */
  const firstSavedMatch = $derived(savedMatches[0] ?? null);

  /**
   * The rail is one tab stop, not one per row: Tab from the filter lands on the
   * selected connection (or the first), and the arrow keys walk from there. A
   * list of forty rows that each take a Tab is not a keyboard path, it is a
   * reason to reach for the mouse.
   */
  const savedRovingId = $derived(
    savedMatches.find((c) => c.id === editingId)?.id ?? savedMatches[0]?.id ?? null,
  );

  /**
   * Whether rows still play their staggered entrance.
   *
   * The stagger is a first-impression flourish, and it is the wrong thing the
   * moment the list becomes a filter result: rows enter and leave on every
   * keystroke, and replaying a 250ms rise with up to 480ms of accumulated delay
   * on each one reads as lag rather than polish. So it retires on the first
   * keystroke and comes back the next time the dialog is opened.
   */
  let savedStagger = $state(true);

  /** Move focus from the filter into the list, so a match can be reached without
   *  leaving the keyboard. Rows carry tabindex, so they take focus directly. */
  function focusFirstSavedRow() {
    const rows = /** @type {HTMLElement[]} */ ([
      ...document.querySelectorAll("[data-conn-row]"),
    ]);
    // `tabIndex` rather than an attribute selector: whether the framework writes
    // the property or the attribute is not something this has to know.
    const row = rows.find((r) => r.tabIndex === 0) ?? rows[0];
    row?.focus();
  }
  let lastId = $state(getLastConnectionId());
  let editingId = $state(/** @type {string|null} */ (null));
  let connecting = $state(/** @type {string|null} */ (null));
  let testing = $state(false);
  let error = $state("");
  let testOk = $state(false);

  // Cancellation token. The Tauri command keeps running, but bumping this makes
  // the in-flight handler ignore its result and unblock the UI immediately.
  let opId = 0;
  function stopOp() {
    opId += 1;
    connecting = null;
    testing = false;
    error = "";
    testOk = false;
  }

  let dbType = $state("postgres");
  // Top-level entry mode: connect manually vs sign in with a hosting provider.
  let entryMode = $state(/** @type {'manual'|'provider'} */ ("manual"));
  // Advanced (SSL / SSH / read-only) disclosure - collapsed by default.
  let name = $state("");
  let host = $state("127.0.0.1");
  let port = $state("5432");
  let database = $state("postgres");
  let user = $state("postgres");
  let password = $state("");
  let ssl = $state(false);
  /**
   * How far TLS verification goes, and what it verifies against.
   *
   * `ssl` alone means "require" - encrypted, certificate unchecked - which is
   * what a managed provider hands you and not what it recommends. `verify-ca`
   * and `verify-full` need a CA to verify against, hence the path beside it.
   * Both travel to the backend as `sslMode`/`sslRootCert` and end up in the
   * connection URL (`sslmode`/`sslrootcert` on Postgres, `ssl-mode`/`ssl-ca` on
   * MySQL).
   * @type {'require' | 'verify-ca' | 'verify-full'}
   */
  let sslMode = $state("require");
  let sslCaPath = $state("");
  let secure = $state(false);
  let encrypt = $state(false);
  let trustCert = $state(true);
  let filePath = $state("");
  let accountId = $state("");
  let databaseId = $state("");
  let apiToken = $state("");
  let libsqlUrl = $state("");
  let libsqlToken = $state("");
  let connectionUri = $state("");
  let uriHint = $state("");
  // Manual-form input mode for URI-capable engines: paste a connection string
  // vs. fill individual fields. UI-only - not tracked as a dirty change.


  // ── Connection options ───────────────────────────────────────────────────────
  let readOnly = $state(false);

  // ── SSH tunnel state ─────────────────────────────────────────────────────────
  let sshEnabled = $state(false);
  let sshHost = $state("");
  let sshPort = $state("22");
  let sshUsername = $state("");
  let sshKeyPath = $state("");
  /**
   * How the tunnel authenticates.
   *
   * Two options, because two are what the transport can actually do: the
   * tunnel is a system `ssh -N -L` process, so an identity file is `-i` and
   * anything else is the agent. A password option would have to drive an
   * interactive prompt (or ship `sshpass`), so it is not offered rather than
   * offered and broken.
   * @type {'key' | 'agent'}
   */
  let sshAuth = $state("key");
  /** `ServerAliveInterval`, seconds. Blank or 0 disables the keepalive. */
  let sshKeepalive = $state("30");

  /**
   * TLS verification levels, in the order they harden.
   *
   * One id set for both engines; `sslPayload` maps them to the engine's own
   * spelling (`verify-full` ↔ MySQL's `VERIFY_IDENTITY`), so the form has one
   * vocabulary and the URL has the right one.
   */
  const SSL_MODES = [
    { value: "require", label: "Require", desc: "Encrypt, do not check the certificate" },
    { value: "verify-ca", label: "Verify CA", desc: "Check the certificate against a CA" },
    { value: "verify-full", label: "Verify full", desc: "Check the CA and that the hostname matches" },
  ];

  /** What the tunnel can authenticate with - see `sshAuth`. */
  const SSH_AUTH_MODES = [
    { value: "key", label: "Key file", desc: "An identity file, passed to ssh as -i" },
    { value: "agent", label: "SSH agent", desc: "Whatever your running agent holds" },
  ];

  /** Panel expansion, independent of whether the feature is switched on. */
  let sslPanelOpen = $state(false);
  let sshPanelOpen = $state(false);

  /** Set when the D1 connection being edited was created by the Cloudflare sign-in. */
  let d1Oauth = $state(false);

  const DEFAULTS = {
    postgres: {
      name: "Local PostgreSQL",
      host: "127.0.0.1",
      port: "5432",
      database: "postgres",
      user: "postgres",
    },
    mysql: {
      name: "Local MySQL",
      host: "127.0.0.1",
      port: "3306",
      database: "mysql",
      user: "root",
    },
    mariadb: {
      name: "Local MariaDB",
      host: "127.0.0.1",
      port: "3306",
      database: "mysql",
      user: "root",
    },
    cockroachdb: {
      name: "Local CockroachDB",
      host: "127.0.0.1",
      port: "26257",
      database: "defaultdb",
      user: "root",
    },
    sqlite: { name: "Local SQLite", filePath: "" },
    "sqlite-memory": { name: "In-Memory SQLite", filePath: ":memory:" },
    libsql: { name: "My Turso DB", libsqlUrl: "", libsqlToken: "" },
    d1: { name: "Cloudflare D1", accountId: "", databaseId: "", apiToken: "" },
    clickhouse: {
      name: "Local ClickHouse",
      host: "127.0.0.1",
      port: "8123",
      database: "default",
      user: "default",
    },
    duckdb: { name: "Local DuckDB", filePath: "" },
    "duckdb-memory": { name: "In-Memory DuckDB", filePath: ":memory:" },
    mssql: {
      name: "Local SQL Server",
      host: "127.0.0.1",
      port: "1433",
      database: "master",
      user: "sa",
    },
    redis: {
      name: "Local Redis",
      host: "127.0.0.1",
      port: "6379",
      database: "0",
    },
  };

  const activeDriver = $derived(
    ALL_DRIVERS.find((d) => d.id === dbType) ?? ALL_DRIVERS[0],
  );
  const isProvider = $derived(PROVIDER_IDS.includes(dbType));
  // D1 shares the Provider tab (via CloudflareLogin) but is not a ProviderConnect id.
  const isProviderTab = $derived(isProvider || dbType === "d1");

  /**
   * Which half of the flow is on screen: 'pick' asks what you're connecting to,
   * 'form' asks for that database's details. Nothing about a connection is shown
   * before it has been chosen - and editing a saved one opens straight at 'form'.
   * @type {'pick' | 'form'}
   */
  let step = $state("pick");


  /** Providers with an account flow, in the order they are offered. */
  const PROVIDER_CARDS = ["neon", "supabase", "prisma", "planetscale", "tidb", "turso", "railway", "nile", "upstash", "d1"];

  /** Names for providers a URI can identify but the catalog has no card for. */
  const PROVIDER_LABELS = { "prisma-postgres": "Prisma Postgres" };

  /** The front page's paste bar. */
  let quickUri = $state("");
  let quickHint = $state("");
  /** @type {HTMLInputElement | null} */
  let quickUriEl = $state(null);

  /**
   * Take a pasted connection string straight to a filled-in form.
   *
   * The engine is in the string, so asking for it first is asking the user to
   * repeat themselves. `detectConnectionUri` reads the scheme (and the host, for
   * a provider), `pickEngine` puts us on that driver's form, and the existing
   * `applyConnectionUri` fills the fields from the same string.
   */
  /** The form's "Import from URL" popover. */
  let importOpen = $state(false);
  let importUri = $state("");

  /**
   * Import from URL, as an action rather than a mode.
   *
   * This was a "Connection string | Fields" switch: two views of one form, one
   * of which hid every field behind a mode you had to switch back out of. A
   * connection string is something you HAVE, once - so it is a button that
   * fills the fields and gets out of the way, which is what every other client
   * calls Import from URL.
   */
  function runImport() {
    const raw = importUri.trim();
    if (!raw) return;
    // A file engine takes a path, and a path is not a URL - hand it straight to
    // the sqlite parser rather than letting the detector call it unreadable.
    if (dbType === "sqlite" || dbType === "duckdb") {
      if (!/^[a-z][a-z0-9+.-]*:/i.test(raw)) {
        filePath = raw;
        flashFields(["filePath"]);
        importUri = "";
        importOpen = false;
        uriHint = "";
        return;
      }
    }
    const hit = detectConnectionUri(raw);
    // A string for another engine switches the engine too - refusing it and
    // making the user go and change the dropdown first would be pedantry.
    if (hit && hit.type !== dbType && !DISABLED_TABS.has(hit.type)) switchDriver(hit.type);
    const ok = applyConnectionUriFrom(raw, hit?.uriType ?? uriTypeFor(dbType));
    if (ok) {
      importUri = "";
      // Left open would mean an empty bar sitting over the fields it just
      // filled, with a success message about work already done.
      importOpen = false;
      uriHint = "";
    }
  }

  /**
   * A connection string in the clipboard is where this dialog usually starts -
   * copied out of a provider dashboard, a .env, or a teammate's message - and
   * the first thing anyone does here is paste it. So clicking into the paste
   * bar pastes it.
   *
   * Strictly: only on a click into the bar (never on open or window focus - on
   * macOS every clipboard read can raise the system paste prompt), only a string
   * that parses as a connection URI, only into an empty bar, and nothing is
   * applied until Continue is pressed. It saves the paste,
   * not the decision. A `.env` line is unwrapped (`DATABASE_URL="postgres://…"`)
   * because that is the form the string is usually copied in.
   */
  /** What the clipboard last put in the bar, so a refill can tell its own text from typing. */
  let clipboardFilled = "";
  /** A string the user cleared away. Putting it back on the next click would be a fight. */
  let clipboardDismissed = "";

  async function prefillFromClipboard() {
    // Typing beats the clipboard, always - and then there is no reason to read it.
    if (!open || step !== "pick") return;
    if (quickUri.trim() && quickUri !== clipboardFilled) return;
    try {
      const raw = (await readClipboardText()).trim();
      if (!raw || raw.length > 2000 || raw.includes("\n")) return;
      const unwrapped = raw
        .replace(/^(?:export\s+)?[A-Za-z_][A-Za-z0-9_]*\s*=\s*/, "")
        .replace(/^["']|["']$/g, "")
        .trim();
      const candidate = detectConnectionUri(raw) ? raw : detectConnectionUri(unwrapped) ? unwrapped : "";
      if (!candidate) return;
      // Anything the user did while the read was in flight wins.
      if (!open || step !== "pick") return;
      if (candidate === quickUri) return;                 // already there
      if (candidate === clipboardDismissed) return;       // they cleared this one away
      // Only ever replaces an empty bar or the text this put there itself.
      // Typing beats the clipboard, always.
      if (quickUri.trim() && quickUri !== clipboardFilled) return;
      quickUri = candidate;
      clipboardFilled = candidate;
      quickHint = "Pasted from your clipboard - press Continue to use it.";
      await tick();
      quickUriEl?.focus();
      quickUriEl?.select?.();
    } catch {
      // No clipboard access (browser dev, or the OS said no): the bar stays empty.
    }
  }

  function useQuickUri() {
    const raw = quickUri.trim();
    quickHint = "";
    if (!raw) return;
    const hit = detectConnectionUri(raw);
    if (!hit) {
      quickHint = "That does not look like a connection string. Try postgres://…, mysql://…, or a path to a .db file.";
      return;
    }
    if (DISABLED_TABS.has(hit.type)) {
      quickHint = `${driverById(hit.type).label} support is coming soon.`;
      return;
    }
    pickEngine(hit.type);
    // The provider's own sign-in is the better path when the string is only an
    // endpoint (no password), but a full URI already has what we need - so fill
    // the form and just say the provider was recognised.
    // `driverById` falls back to the first driver for an unknown id, so an
    // provider that is not itself a catalog entry (turso) must not go through it
    // - it would claim the string was PostgreSQL.
    if (hit.provider) {
      const known = ALL_DRIVERS.find((d) => d.id === hit.provider);
      quickHint = `Recognised a ${known?.label ?? PROVIDER_LABELS[hit.provider] ?? hit.provider} endpoint.`;
    }
    const ok = applyConnectionUriFrom(raw, hit.uriType);
    if (!ok && !quickHint) quickHint = "Could not read that string.";
    quickUri = "";
    // Used. Coming back to the picker should not hand it over again.
    clipboardDismissed = raw;
  }

  /** A card in step 1 was chosen: set the engine and move on. @param {string} id */
  function pickEngine(id) {
    if (DISABLED_TABS.has(id)) return;
    entryMode =
      PROVIDER_IDS.includes(id) || id === "d1" ? "provider" : "manual";
    switchDriver(id);
    step = "form";
    // Choosing what to connect to is navigation, not an edit - re-baseline so
    // opening the picker and closing again doesn't ask about discarding changes
    // nobody made. Only editing a saved connection can be dirty.
    if (!editingId) baseline = snapshot();
  }

  /** Back to the picker, keeping whatever has been typed so far. */
  // The mouse's back/forward buttons, inside the dialog.
  //
  // StudioShell owns these globally but bails while a modal is open, because
  // walking the app's tab history behind a dialog is not what the button means
  // there. It does mean something here though: back is the same step the header
  // arrow takes, form → engine picker. Registered in the capture phase and
  // stopped, so the global handler never sees it, and `preventDefault` runs even
  // when there is nowhere to go - left alone the webview walks its own document
  // history, which in a Tauri window navigates away from the app.
  $effect(() => {
    if (!open) return;
    /** @param {MouseEvent} e */
    function onMouseNav(e) {
      if (e.button !== 3 && e.button !== 4) return;
      e.preventDefault();
      e.stopPropagation();
      if (e.button !== 3) return; // nothing to go forward to
      if (step === "form") backToPick();
      else requestClose();
    }
    /** Swallow the paired auxclick/mouseup so the webview cannot act on them. */
    function swallowAux(/** @type {MouseEvent} */ e) {
      if (e.button === 3 || e.button === 4) {
        e.preventDefault();
        e.stopPropagation();
      }
    }
    document.addEventListener("mousedown", onMouseNav, { capture: true });
    document.addEventListener("auxclick", swallowAux, { capture: true });
    document.addEventListener("mouseup", swallowAux, { capture: true });
    return () => {
      document.removeEventListener("mousedown", onMouseNav, { capture: true });
      document.removeEventListener("auxclick", swallowAux, { capture: true });
      document.removeEventListener("mouseup", swallowAux, { capture: true });
    };
  });

  function backToPick() {
    error = "";
    testOk = false;
    step = "pick";
    engineQuery = "";
  }

  /**
   * "New connection" - straight to the form, on the commonest engine.
   *
   * There was a page in between: seventeen drivers under five headings, laid
   * out as a grid of icons, whose entire job was to set one field. That field
   * is a dropdown on the form now - searchable, in reading order with the rest
   * of the connection - so the step is gone and so is the grid.
   */
  function newConnectionForm() {
    error = "";
    testOk = false;
    quickHint = "";
    pickEngine("postgres");
  }

  /** Every driver, for the form's engine dropdown. */
  const engineItems = $derived(
    CATEGORIES.flatMap((cat) =>
      cat.drivers.map((d) => ({
        value: d.id,
        label: d.label,
        // The category and the blurb are searchable too, so "cloud", "edge" or
        // "serverless" finds what the label alone does not.
        keywords: [d.label, cat.label, d.desc, d.id],
        group: cat.label,
        desc: d.desc,
        disabled: DISABLED_TABS.has(d.id),
      })),
    ),
  );

  // Engines that expose the "Connection string | Manual fields" toggle.
  /**
   * Engines whose form offers the "Connection string | Fields" switch.
   *
   * ClickHouse and SQL Server used to carry their own URI input inside their
   * field forms instead - a second way to do the same thing, with its own
   * label, its own Parse button and its own copy of the hint markup, in an
   * engine-specific branch. They use the switch now, so there is one import
   * path per form and one on the front page, rather than four.
   */
  const URI_TOGGLE_ENGINES = [
    "postgres", "cockroachdb", "mysql", "mariadb", "clickhouse", "mssql",
    // Every dialect that has a URL form, not just the SQL ones: a Redis or
    // Turso string is the same paste, and refusing it here sent people to fill
    // four fields by hand from a string they already had.
    "redis", "libsql", "sqlite", "duckdb",
  ];
  const hasFieldToggle = $derived(URI_TOGGLE_ENGINES.includes(dbType));

  // Briefly ring-highlight the fields a parsed connection string just filled in.
  let flashedFields = $state(/** @type {Set<string>} */ (new Set()));
  let flashTimer;
  const flashCls = "ring-2 ring-success/50";
  function flashFields(keys) {
    flashedFields = new Set(keys);
    clearTimeout(flashTimer);
    flashTimer = setTimeout(() => {
      flashedFields = new Set();
    }, 1000);
  }
  onDestroy(() => {
    clearTimeout(flashTimer);
    clearTimeout(justSavedTimer);
  });

  function sshPayload() {
    if (!sshEnabled || !sshHost.trim() || !sshUsername.trim()) return undefined;
    return {
      host: sshHost.trim(),
      port: Number(sshPort) || 22,
      username: sshUsername.trim(),
      // Agent auth IS an empty identity path, as far as `ssh` is concerned.
      privateKeyPath: sshAuth === "agent" ? "" : sshKeyPath.trim(),
      keepalive: Math.max(0, Math.min(3600, Number(sshKeepalive) || 0)),
    };
  }

  /**
   * The TLS pair for a host engine, in that engine's spelling.
   *
   * Only sent when TLS is on: `sslMode` overrides the plain boolean in the
   * backend, so passing it while `ssl` is false would quietly re-enable it.
   * @param {'postgres' | 'mysql'} family
   */
  function sslPayload(family) {
    if (!ssl) return {};
    const mode =
      family === "mysql"
        ? { require: "required", "verify-ca": "verify_ca", "verify-full": "verify_identity" }[sslMode]
        : sslMode;
    /** @type {Record<string, string>} */
    const out = { sslMode: mode };
    if (sslMode !== "require" && sslCaPath.trim()) out.sslRootCert = sslCaPath.trim();
    return out;
  }

  function formPayload() {
    const ssh = sshPayload();
    // Naming a connection is optional: an empty Name field takes the derived one
    // rather than saving a row that reads "Unnamed" in the sidebar forever.
    const nm = name.trim() || autoName;
    if (dbType === "sqlite" || dbType === "sqlite-memory")
      return {
        type: "sqlite",
        name: nm,
        filePath: dbType === "sqlite-memory" ? ":memory:" : filePath,
      };
    if (dbType === "libsql")
      return {
        type: "libsql",
        name: nm,
        url: libsqlUrl,
        authToken: libsqlToken || undefined,
      };
    if (dbType === "d1")
      return {
        type: "d1",
        name: nm,
        accountId,
        databaseId,
        apiToken,
        ...(d1Oauth && { oauth: true }),
      };
    if (dbType === "mysql" || dbType === "mariadb")
      return {
        type: dbType,
        name: nm,
        host,
        port,
        database,
        user,
        password,
        ssl,
        ...sslPayload("mysql"),
        ...(ssh && { ssh }),
      };
    if (dbType === "cockroachdb")
      return {
        type: "cockroachdb",
        name: nm,
        host,
        port,
        database,
        user,
        password,
        ssl,
        ...sslPayload("postgres"),
        ...(ssh && { ssh }),
      };
    if (dbType === "clickhouse")
      return {
        type: "clickhouse",
        name: nm,
        host,
        port,
        database,
        user,
        password,
        secure,
      };
    if (dbType === "duckdb" || dbType === "duckdb-memory")
      return {
        type: "duckdb",
        name: nm,
        filePath: dbType === "duckdb-memory" ? ":memory:" : filePath,
      };
    if (dbType === "mssql")
      return {
        type: "mssql",
        name: nm,
        host,
        port,
        database,
        user,
        password,
        encrypt,
        trustCert,
      };
    if (dbType === "redis")
      return {
        type: "redis",
        name: nm,
        host,
        port,
        password,
        db: Number(database) || 0,
        tls: secure,
      };
    return {
      type: "postgres",
      name: nm,
      host,
      port,
      database,
      user,
      password,
      ssl,
      ...sslPayload("postgres"),
      ...(ssh && { ssh }),
    };
  }

  /**
   * Report a failure.
   *
   * The alert is a toast, not a panel wedged into the form: a connection error
   * arrives while you are looking at the field that caused it, and a block that
   * grows the page (title + advice + button + raw driver text) pushed the fields
   * around at exactly the wrong moment. `error` is still set, because the footer
   * chip and the diagnosis both read it.
   *
   * @param {string} msg
   */
  function failWith(msg) {
    error = msg;
    // errorFix is derived from `error`, so it re-reads against the new message.
    const fix = errorFix;
    toast.error(fix ? fix.title : "Couldn't connect", {
      description: fix ? fix.hint : msg,
      // Unrecognised failures show the driver's own words, so they get the
      // monospace treatment the toast already has for raw output.
      ...(fix ? {} : { code: true }),
      duration: 9000,
      ...(fix?.action
        ? { action: { label: fix.actionLabel, onClick: fix.action } }
        : {}),
    });
  }

  /** Normalize a thrown value into a concise, user-facing message (no "Error:"
   * prefix, no raw object noise). */
  function friendlyError(/** @type {unknown} */ e) {
    let msg =
      typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
    msg = msg.replace(/^Error:\s*/i, "").trim();
    return msg || "Something went wrong. Please try again.";
  }

  function resetForm(conn) {
    editingId = conn?.id ?? null;
    if (conn) {
      const t =
        conn.filePath === ":memory:" &&
        (conn.type === "sqlite" || conn.type === "duckdb")
          ? `${conn.type}-memory`
          : (conn.type ?? "postgres");
      dbType = t;
      name = conn.name ?? "";
      host = conn.host ?? "127.0.0.1";
      port = String(conn.port ?? 5432);
      // Redis persists its logical DB index / TLS under `db` / `tls`; the form
      // reuses the shared `database` / `secure` fields, so map them back on edit.
      database =
        conn.database ?? (conn.db != null ? String(conn.db) : "postgres");
      user = conn.user ?? "postgres";
      password = conn.password ?? "";
      ssl = Boolean(conn.ssl);
      secure = Boolean(conn.secure ?? conn.tls);
      encrypt = Boolean(conn.encrypt);
      trustCert = conn.trustCert ?? true;
      filePath = conn.filePath ?? "";
      accountId = conn.accountId ?? "";
      databaseId = conn.databaseId ?? "";
      apiToken = conn.apiToken ?? "";
      d1Oauth = !!conn.oauth;
      libsqlUrl = conn.url ?? "";
      libsqlToken = conn.authToken ?? "";
      const s = conn.ssh;
      sshEnabled = !!s?.host;
      sshHost = s?.host ?? "";
      sshPort = String(s?.port ?? 22);
      sshUsername = s?.username ?? "";
      sshKeyPath = s?.privateKeyPath ?? "";
      // A saved tunnel with no identity path was using the agent.
      sshAuth = s?.privateKeyPath ? "key" : "agent";
      sshKeepalive = String(s?.keepalive ?? 30);
      sslMode = /** @type {any} */ (
        conn.sslMode === "verify_ca" ? "verify-ca"
        : conn.sslMode === "verify_identity" ? "verify-full"
        : conn.sslMode || "require"
      );
      sslCaPath = conn.sslRootCert ?? "";
      sslPanelOpen = !!conn.ssl;
      sshPanelOpen = !!s?.host;
      readOnly = conn.readOnly ?? false;
    } else {
      dbType = "postgres";
      name = "";
      host = "127.0.0.1";
      port = "5432";
      database = "postgres";
      user = "postgres";
      password = "";
      ssl = false;
      secure = false;
      encrypt = false;
      trustCert = true;
      filePath = "";
      accountId = "";
      databaseId = "";
      apiToken = "";
      d1Oauth = false;
      libsqlUrl = "";
      libsqlToken = "";
      sshEnabled = false;
      sshHost = "";
      sshPort = "22";
      sshUsername = "";
      sshKeyPath = "";
      sshAuth = "key";
      sshKeepalive = "30";
      sslMode = "require";
      sslCaPath = "";
      sslPanelOpen = false;
      sshPanelOpen = false;
      readOnly = false;
    }
    entryMode =
      PROVIDER_IDS.includes(dbType) || dbType === "d1" ? "provider" : "manual";
    // An existing connection already answered "what are you connecting to", so it
    // opens on its details. A new one starts at the choice.
    step = conn ? "form" : "pick";
    flashedFields = new Set();
    error = "";
    testOk = false;
    connectionUri = "";
    uriHint = "";
    baseline = snapshot();
  }

  /**
   * A provider adapter resolved a database (with a password if it needed one):
   * build a SavedConnection and connect immediately via connectWith - no detour
   * through the manual form, so picking a project just connects. Runs the same
   * connect/save/close path as any other connection.
   * @param {import('$lib/providers.js').ProviderConnection} conn
   */
  async function connectProviderConnection(conn) {
    error = "";
    // Credentials reused from a saved connection can have been revoked in the
    // provider's console since. They used to be probed first with a full test
    // connect, then connected again for real: two complete handshakes on every
    // reuse, which against a far region (TiDB in Tokyo, Nile in us-west-2) was
    // most of the wait. Now the real connect goes first, and only a rejected
    // login falls back to minting fresh credentials - silently, with no auth
    // toast ahead of the retry.
    if (conn.reuse) {
      const ref = conn.providerRef;
      await connectWith(
        // The saved entry as it is, with this panel's read-only choice.
        { ...conn.reuse, providerRef: ref, readOnly: readOnly || conn.reuse.readOnly || undefined },
        {
          onAuthFailure: async () => {
            const fresh = await providerBuildConnection(dbType, ref);
            // Supabase never returns a password: a rejected saved one can't be
            // replaced from here, so say so rather than connect with none.
            if (fresh.needs_password) {
              failWith(`The saved password for ${conn.reuse.name} was rejected. Pick the database again and enter the current password.`);
              return;
            }
            await connectProviderResolved({ ...fresh, providerRef: ref });
          },
        },
      );
      return;
    }
    await connectProviderResolved(conn);
  }

  /**
   * Build a SavedConnection from a resolved provider spec and connect.
   * @param {import('$lib/providers.js').ProviderConnection} conn
   * @param {{ onAuthFailure?: () => Promise<void> }} [opts]
   */
  async function connectProviderResolved(conn, opts = {}) {
    error = "";
    // dbType is the provider id while the provider flow is showing - tag the
    // connection with it so the status bar can offer switching to the account's
    // other databases later.
    const providerId = PROVIDER_IDS.includes(dbType) ? dbType : undefined;
    // libsql has no host/port/user: the adapter hands over the `libsql://` URL
    // in `host` and the database token in `password`.
    if (conn.db_type === "libsql") {
      const existing = saved.find((s) => s.type === "libsql" && s.url === conn.host);
      await connectWith({
        id: existing?.id ?? newConnectionId(),
        type: "libsql",
        name: conn.name,
        url: conn.host,
        authToken: conn.password || undefined,
        provider: providerId,
        providerRef: conn.providerRef,
        readOnly: readOnly || undefined,
      }, opts);
      return;
    }
    // Redis (Upstash, Railway): the saved shape has `db` and `tls`, not a
    // database name and `ssl`.
    if (conn.db_type === "redis") {
      const existing = saved.find(
        (s) => s.type === "redis" && s.host === conn.host && s.port === conn.port,
      );
      await connectWith({
        id: existing?.id ?? newConnectionId(),
        type: "redis",
        name: conn.name,
        host: conn.host,
        port: conn.port,
        password: conn.password,
        db: Number(conn.database) || 0,
        tls: conn.ssl,
        provider: providerId,
        providerRef: conn.providerRef,
        readOnly: readOnly || undefined,
      }, opts);
      return;
    }
    const type = conn.db_type === "mysql" ? "mysql" : "postgres";
    // Reuse an existing saved entry for this exact database (host + user) instead
    // of piling up duplicates - connectWith upserts it, keeping the saved password.
    const existing = saved.find(
      (s) =>
        s.host === conn.host && s.user === conn.username && s.type === type,
    );
    await connectWith({
      id: existing?.id ?? newConnectionId(),
      type,
      name: conn.name,
      host: conn.host,
      port: conn.port,
      database: conn.database,
      user: conn.username,
      password: conn.password,
      ssl: conn.ssl,
      provider: providerId,
      providerRef: conn.providerRef,
      readOnly: readOnly || undefined,
    }, opts);
  }

  /**
   * Cloudflare D1 picked from the Provider tab: fill the D1 fields and connect
   * immediately, mirroring the one-click flow of the other hosting providers.
   * @param {{accountId: string, databaseId: string, databaseName: string, token: string}} info
   */
  async function connectD1Selection(info) {
    error = "";
    accountId = info.accountId;
    databaseId = info.databaseId;
    apiToken = info.token;
    if (!name || name === "Cloudflare D1") name = info.databaseName;
    // Reuse an existing saved entry for this exact D1 database instead of piling
    // up duplicates - connectWith upserts it.
    const existing = saved.find(
      (s) => s.type === "d1" && s.databaseId === info.databaseId,
    );
    await connectWith({
      id: existing?.id ?? newConnectionId(),
      type: "d1",
      name: info.databaseName,
      accountId: info.accountId,
      databaseId: info.databaseId,
      apiToken: info.token,
      // The token is an OAuth access token with a ~10 minute life, so mark where
      // it came from: reconnecting later has to mint a fresh one rather than
      // replay this snapshot (see d1Call in api.js).
      oauth: true,
      readOnly: readOnly || undefined,
    });
  }

  /**
   * Default port per engine. One table, read by both the engine picker and the
   * save path - they used to carry separate copies that already disagreed about
   * ClickHouse (8123 here, TLS-aware there), so which port a saved row ended up
   * with depended on whether you had touched the picker.
   * Engines with no port at all (SQLite, DuckDB, D1, libSQL) are absent on
   * purpose: `undefined` is what clears the field.
   */
  const DEFAULT_PORTS = {
    postgres: 5432,
    mysql: 3306,
    mariadb: 3306,
    cockroachdb: 26257,
    clickhouse: 8123,
    mssql: 1433,
    redis: 6379,
  };
  /** @param {string} id @param {boolean} [isSecure] */
  function defaultPortFor(id, isSecure = secure) {
    if (id === "clickhouse") return isSecure ? 8443 : 8123;
    return DEFAULT_PORTS[id];
  }

  /**
   * Default database and login per engine. The port already worked this way; the
   * other two did not, so picking MySQL left "postgres" sitting in Database and
   * Username - a Postgres default under a MySQL header, which is not a form you
   * can trust. Absent = the engine has no sensible default and the field clears.
   * @type {Record<string, { database?: string, user?: string }>}
   */
  const ENGINE_DEFAULTS = {
    postgres:    { database: "postgres", user: "postgres" },
    cockroachdb: { database: "defaultdb", user: "root" },
    mysql:       { database: "", user: "root" },
    mariadb:     { database: "", user: "root" },
    clickhouse:  { database: "default", user: "default" },
    mssql:       { database: "master", user: "sa" },
    redis:       { database: "0", user: "" },
  };

  function switchDriver(id) {
    const prevPort = defaultPortFor(dbType);
    const prev = ENGINE_DEFAULTS[dbType] ?? {};
    const next = ENGINE_DEFAULTS[id] ?? {};
    dbType = id;
    // Re-default a field only when it still holds the *previous* engine's
    // default. Anything typed by hand is the user's and survives a trip through
    // the engine picker and back; every switch used to overwrite the port, and
    // never touched database or username at all - which is how a MySQL form
    // ended up asking for the "postgres" database as user "postgres".
    // An engine with no default clears the field rather than inheriting a stale
    // value from the last one.
    if (port === "" || String(prevPort ?? "") === String(port)) {
      const p = defaultPortFor(id);
      port = p != null ? String(p) : "";
    }
    if (database === "" || database === prev.database) database = next.database ?? "";
    if (user === "" || user === prev.user) user = next.user ?? "";
    if (id === "sqlite-memory" || id === "duckdb-memory") filePath = ":memory:";
    if (id === "duckdb") filePath = "";
    error = "";
    testOk = false;
    connectionUri = "";
    uriHint = "";
  }


  /**
   * Which of the five parsers `dbType` needs.
   *
   * Was written out twice - here and in `studioConnection` - and now three
   * times over, with the front page's paste bar. One function.
   * @param {string} t
   * @returns {'postgres'|'sqlite'|'mysql'|'mssql'|'clickhouse'}
   */
  function uriTypeFor(t) {
    if (t === "sqlite" || t === "sqlite-memory" || t === "duckdb" || t === "duckdb-memory") return "sqlite";
    if (t === "mysql" || t === "mariadb") return "mysql";
    if (t === "mssql") return "mssql";
    if (t === "clickhouse") return "clickhouse";
    // Redis and LibSQL have their own shapes: a `redis://` string read by the
    // Postgres parser loses the database index and mistakes a bare password for
    // a username, and a Turso URL carries its token in the query string.
    if (t === "redis") return "redis";
    if (t === "libsql") return "libsql";
    return "postgres";
  }

  function applyConnectionUri() {
    return applyConnectionUriFrom(connectionUri, uriTypeFor(dbType));
  }

  /**
   * Fill the form from a connection string.
   * @param {string} raw
   * @param {'postgres'|'sqlite'|'mysql'|'mssql'|'clickhouse'} uriType
   */
  function applyConnectionUriFrom(raw, uriType) {
    uriHint = "";
    const parsed = parseConnectionUri(uriType, raw);
    if (!parsed) return false;
    if ("error" in parsed) {
      uriHint = parsed.error;
      return false;
    }
    // Redis: host, port, logical database, TLS and a password that may have
    // arrived without a username.
    if ("tls" in parsed) {
      host = parsed.host;
      port = parsed.port;
      database = parsed.db;
      user = parsed.user;
      password = parsed.password;
      secure = parsed.tls;
      flashFields(["host", "port", "database", "user", "password", "secure"]);
      uriHint = "Fields updated from URL";
      return true;
    }
    // LibSQL / Turso: the URL, with any `authToken` moved to its own field.
    if ("authToken" in parsed) {
      libsqlUrl = parsed.url;
      if (parsed.authToken) libsqlToken = parsed.authToken;
      flashFields(["libsqlUrl", "libsqlToken"]);
      uriHint = "Fields updated from URL";
      return true;
    }
    if (
      (dbType === "sqlite" || dbType === "sqlite-memory" || dbType === "duckdb" || dbType === "duckdb-memory") &&
      "filePath" in parsed
    ) {
      filePath = parsed.filePath;
      flashFields(["filePath"]);
      uriHint = "Fields updated from URI";
      return true;
    }
    if ("host" in parsed) {
      host = parsed.host;
      port = parsed.port;
      database = parsed.database;
      user = parsed.user;
      password = parsed.password;
      const changed = ["host", "port", "database", "user", "password"];
      // Each engine carries its own TLS shape; apply whichever the parser returned.
      if ("encrypt" in parsed) {
        encrypt = parsed.encrypt;
        trustCert = parsed.trustCert;
        changed.push("encrypt", "trustCert");
      } else if ("secure" in parsed) {
        secure = parsed.secure;
        changed.push("secure");
      } else {
        ssl = parsed.ssl;
        changed.push("ssl");
      }
      flashFields(changed);
      uriHint = "Fields updated from URI";
      return true;
    }
    return false;
  }

  /**
   * A readable stand-in for an empty Name field - shown as its placeholder and
   * used verbatim on save, so naming a connection stays optional.
   */
  const autoName = $derived.by(() => {
    if (dbType === "sqlite-memory" || dbType === "duckdb-memory")
      return "Scratch database";
    if (dbType === "sqlite" || dbType === "duckdb")
      return (
        filePath
          .split(/[\\/]/)
          .pop()
          ?.replace(/\.(sqlite3?|db|duckdb)$/i, "") || activeDriver.label
      );
    if (dbType === "libsql")
      return (
        libsqlUrl.replace(/^\w+:\/\//, "").split(".")[0] || activeDriver.label
      );
    if (dbType === "d1") return "Cloudflare D1";
    if (host.trim())
      return `${database.trim() || activeDriver.label}@${host.trim()}`;
    return activeDriver.label;
  });

  /** Placeholder for the paste bar - the shape this engine actually accepts. */
  const URI_PLACEHOLDERS = {
    mysql: "mysql://user:pass@host:3306/db",
    mariadb: "mysql://user:pass@host:3306/db",
    cockroachdb: "postgresql://user:pass@host:26257/defaultdb",
    clickhouse: "clickhouse://user:pass@host:8123/db",
    mssql: "sqlserver://user:pass@host:1433;database=db",
    redis: "rediss://:password@host:6379/0",
    libsql: "libsql://db-org.turso.io?authToken=…",
    sqlite: "/path/to/app.db",
    duckdb: "/path/to/warehouse.duckdb",
  };
  const uriPlaceholder = $derived(
    URI_PLACEHOLDERS[dbType] ?? "postgresql://user:pass@host:5432/db",
  );

  /** @param {string} id */
  function focusField(id) {
    // Some engine forms prefix their field ids (cn-ch-*, cn-mssql-*, cn-redis-*);
    // fall through to the visible engine's variant so the one-click error fixes
    // land on every form instead of silently no-oping.
    const suffix = id.replace(/^cn-/, "");
    for (const cid of [
      id,
      ...["ch", "mssql", "redis"].map((p) => `cn-${p}-${suffix}`),
    ]) {
      const el = /** @type {HTMLInputElement | null} */ (
        document.getElementById(cid)
      );
      if (el) {
        el.focus();
        el.select?.();
        return;
      }
    }
  }

  /**
   * Turn a driver error into something to *do*.
   *
   * A raw driver message is the least useful moment in the whole flow - it is
   * where people give up and close the dialog. The raw text still shows, but
   * above it goes a plain reading of what failed, and a one-click fix wherever
   * there is an obvious one.
   */
  const errorFix = $derived.by(() => {
    const e = (error || "").toLowerCase();
    if (!e) return null;
    // A REST 401 comes first: `auth.*error` used to swallow "401 Unauthorized:
    // …\"errors\":[…]" and report a wrong *password* for a token that had simply
    // expired, which sends people to a field that isn't the problem.
    if (/\b40[13]\b|unauthorized|invalid api token/.test(e)) {
      const cf = dbType === "d1";
      return {
        title: cf
          ? "Cloudflare rejected the saved token"
          : "The server rejected the credentials",
        hint: cf
          ? "The access token from your Cloudflare sign-in has expired. Signing in again mints a fresh one."
          : "The endpoint answered but refused the token or key it was given.",
        actionLabel: cf ? "Reconnect Cloudflare" : "Check credentials",
        action: cf
          ? () => {
              entryMode = "provider";
              switchDriver("d1");
              step = "form";
            }
          : () =>
              focusField(dbType === "libsql" ? "cn-libsql-token" : "cn-pass"),
      };
    }
    if (
      /password authentication failed|authentication failed|access denied|invalid credentials|login failed/.test(
        e,
      )
    )
      return {
        title: "The username or password was rejected",
        hint: "The server answered, so the address is right - only the credentials were refused.",
        actionLabel: "Check password",
        action: () => focusField("cn-pass"),
      };
    if (
      /does not support ssl|ssl.*required|requires ssl|sslmode|tls.*required/.test(
        e,
      )
    )
      return {
        title: "This server requires an encrypted connection",
        hint: "Turn on SSL / TLS and try again.",
        actionLabel: "Enable SSL & retry",
        action: () => {
          ssl = true;
          void handleTest();
        },
      };
    if (
      /database ".*" does not exist|unknown database|no such database|database .* not found/.test(
        e,
      )
    )
      return {
        title: "That database doesn't exist on the server",
        hint: 'Check the name - on PostgreSQL the default database is usually "postgres".',
        actionLabel: "Edit database",
        action: () => focusField("cn-db"),
      };
    if (
      /connection refused|econnrefused|timed out|timeout|no route to host|network is unreachable|connection reset/.test(
        e,
      )
    )
      // A provider connect has no host field on screen: name the address it
      // actually dialled, and say what the driver said, instead of pointing at
      // the empty manual form ("is 127.0.0.1: the right host").
      return failTarget
        ? {
            title: `Couldn't reach ${failTarget.host}`,
            hint: `${failTarget.host}:${failTarget.port} did not answer. ${String(e).slice(0, 200)}`,
          }
        : {
            title: "Nothing answered at that address",
            hint: `Is the server running, and is ${host}:${port} the right host and port?`,
            actionLabel: "Edit host",
            action: () => focusField("cn-host"),
          };
    if (
      /name or service not known|nodename nor servname|getaddrinfo|failed to lookup|dns/.test(
        e,
      )
    )
      return {
        title: "That host name doesn't resolve",
        hint: "Check the spelling, or use an IP address instead.",
        actionLabel: "Edit host",
        action: () => focusField("cn-host"),
      };
    return null;
  });

  /**
   * The one line under a connection's name, and it answers one question: which
   * database does this open? It used to print source · image · host, which is
   * the connection string in three parts - at 11px in a 220px rail the host was
   * always truncated mid-word, and every one of those fields is already on the
   * form this row opens.
   */
  function connSubtitle(conn) {
    if (conn.type === "sqlite" || conn.type === "duckdb") {
      if (conn.filePath === ":memory:") return "in-memory";
      return String(conn.filePath ?? "").split(/[\\/]/).pop() || "—";
    }
    if (conn.type === "libsql") {
      try { return new URL(conn.url).pathname.replace(/^\//, "") || conn.url; }
      catch { return conn.url || "—"; }
    }
    if (conn.type === "d1") return conn.database || conn.name || "—";
    // Redis has no database name worth showing; where it lives is the useful bit.
    if (conn.type === "redis") return conn.host ? `${conn.host}${conn.port ? `:${conn.port}` : ""}` : "—";
    return conn.database || "—";
  }

  function relativeTime(ts) {
    if (!ts) return "";
    const s = (Date.now() - ts) / 1000;
    if (s < 60) return "just now";
    if (s < 3600) return `${Math.floor(s / 60)}m ago`;
    if (s < 86400) return `${Math.floor(s / 3600)}h ago`;
    return new Date(ts).toLocaleDateString();
  }

  $effect(() => {
    if (!open) return;
    untrack(() => {
      saved = loadSavedConnections().sort(byLastConnected);
      lastId = getLastConnectionId();
      resetForm(null);
      duplicatePrompt = null;
      duplicateAck = false;
      clipboardFilled = "";
      clipboardDismissed = "";
      // Arriving from an engine chip skips the picker - that choice is already made.
      if (initialEngine) { pickEngine(initialEngine); initialEngine = ""; }
      engineQuery = "";
      quickUri = "";
      quickHint = "";
      void refreshLocal();
      // The paste bar takes focus: the modal opens, you paste, you press Enter.
      // Focus alone does not read the clipboard - only a click into the bar does.
      // Only on the front page - a saved connection opens straight into its form.
      void tick().then(() => { if (step === "pick") quickUriEl?.focus(); });
    });
  });

  // Deleting also clears the connection's history, saved queries, charts and
  // chats (purgeConnectionData), so it is asked first rather than done on click.
  /** @type {{ id: string, name: string, fromKeyboard: boolean } | null} */
  let pendingDelete = $state(null);
  let confirmDeleteOpen = $state(false);

  /** @param {string} id @param {boolean} [fromKeyboard] */
  function handleDelete(id, fromKeyboard = false) {
    const conn = saved.find((c) => c.id === id);
    pendingDelete = { id, name: conn?.name || conn?.database || conn?.host || conn?.filePath || "this connection", fromKeyboard };
    confirmDeleteOpen = true;
  }

  function confirmDelete() {
    if (!pendingDelete) return;
    const { id, fromKeyboard } = pendingDelete;
    pendingDelete = null;
    saved = removeConnection(id).sort(byLastConnected);
    if (id === lastId) {
      lastId = null;
      setLastConnectionId(null);
    }
    if (editingId === id) resetForm(null);
    onremoved(id);
    // The row that had focus no longer exists - hand it to whatever took its
    // place rather than letting it fall back to the document.
    if (fromKeyboard) {
      void tick().then(() => {
        if (savedMatches.length) focusFirstSavedRow();
        else savedSearchEl?.focus();
      });
    }
  }

  /** Open `conn` with the driver its `type` calls for. */
  async function openConnection(conn) {
    // Every connect from this dialog is user-initiated, so it is exactly what
    // the "ask when connecting" PIN setting is about. A no-op when no PIN is set.
    if (!(await requireUnlock("Unlock to connect to a database"))) {
      throw new Error("Cancelled - the PIN was not entered.");
    }
    if (conn.type === "sqlite") return connectSqlite(conn);
    if (conn.type === "d1") return connectD1(conn);
    if (conn.type === "libsql") return connectLibSql(conn);
    if (conn.type === "mysql" || conn.type === "mariadb")
      return connectMysql(conn);
    if (conn.type === "clickhouse") return connectClickhouse(conn);
    if (conn.type === "duckdb") return connectDuckdb(conn);
    if (conn.type === "mssql") return connectMssql(conn);
    if (conn.type === "redis") return connectRedis(conn);
    return connectPostgres(conn);
  }

  /** The address a non-form connect dialled, for the unreachable hint. @type {{ host: string, port: number } | null} */
  let failTarget = $state(null);

  /** A driver error that means "these credentials are no good", on any engine. */
  const AUTH_FAILURE =
    /password authentication failed|authentication failed|access denied|invalid password|28P01|\b1045\b|role ".*" does not exist/i;

  /**
   * @param {any} conn
   * @param {{ onAuthFailure?: () => Promise<void> }} [opts] run instead of the
   *   error toast when the credentials are rejected (reused provider credentials
   *   that were revoked: mint fresh ones and try once more).
   */
  async function connectWith(conn, opts = {}) {
    const myOp = ++opId;
    connecting = conn.id;
    error = "";
    // Provider and saved connections dial an address that isn't in the form.
    failTarget = conn.provider && conn.host ? { host: conn.host, port: conn.port } : null;
    try {
      await openConnection(conn);
      if (myOp !== opId) return; // cancelled by the user
      const updated = { ...conn, lastConnectedAt: Date.now() };
      saved = upsertConnection(updated).sort(byLastConnected);
      setLastConnectionId(conn.id);
      open = false;
      await onconnected(updated, conn.id);
    } catch (e) {
      if (myOp !== opId) return;
      if (opts.onAuthFailure && AUTH_FAILURE.test(String(e))) {
        await opts.onAuthFailure();
        return;
      }
      failWith(friendlyError(e));
    } finally {
      if (myOp === opId) connecting = null;
    }
  }

  // ── Studios running on this machine ─────────────────────────────────────────
  // `prisma studio` / `drizzle-kit studio` are already pointed at a database, and
  // the backend reads which one out of that project's own schema/config. So they
  // appear here as one-click targets: nothing to type, and nothing left behind -
  // the row exists only while the studio does.

  /**
   * Free-text filter over the detected local targets.
   *
   * It used to filter a grid of seventeen driver tiles as well; that grid is a
   * dropdown on the form now, with its own search, so this is only ever a
   * filter over what is running on this machine - and with nothing to type into
   * on the front page, it stays empty unless a caller sets it.
   */
  let engineQuery = $state("");

  /** Same rule as the rail's `savedStagger`: the local-target cards are filtered
   *  by this search, so their entrance stagger has to stop once it would be
   *  replaying on every keystroke rather than introducing the panel. */
  let engineStagger = $state(true);

  // A fresh open is a fresh first impression: the rail's entrance stagger is
  // armed again, and a filter left over from last time is not what you want to
  // be looking at.
  $effect(() => {
    if (!open) return;
    savedStagger = true;
    engineStagger = true;
    savedQuery = "";
  });

  // ── Connections rail: width + collapsed state, remembered ──────────────────
  const RAIL_KEY = "stroke:conn-rail";
  const RAIL_MIN = 260;
  const RAIL_MAX = 520;
  const RAIL_DEFAULT = 340;

  function loadRail() {
    try {
      const raw = JSON.parse(localStorage.getItem(RAIL_KEY) ?? "{}");
      return {
        width: Math.min(
          RAIL_MAX,
          Math.max(RAIL_MIN, Number(raw.width) || RAIL_DEFAULT),
        ),
        open: raw.open !== false,
      };
    } catch {
      return { width: RAIL_DEFAULT, open: true };
    }
  }
  const _rail = loadRail();
  let railWidth = $state(_rail.width);
  let railOpen = $state(_rail.open);
  function saveRail() {
    try {
      localStorage.setItem(
        RAIL_KEY,
        JSON.stringify({ width: railWidth, open: railOpen }),
      );
    } catch {
      /* ignore */
    }
  }
  let railDragStart = 0;

  let studios = $state(
    /** @type {import('$lib/api.js').DetectedStudio[]} */ ([]),
  );
  let dockerDbs = $state(
    /** @type {import('$lib/api.js').DockerDatabase[]} */ ([]),
  );
  let machineDbs = $state(
    /** @type {import('$lib/api.js').MachineDatabase[]} */ ([]),
  );
  let localPhase = $state(/** @type {'idle'|'scanning'|'done'} */ ("idle"));

  /**
   * ⌘R / Ctrl+R - rescan. Reloads the saved list as well as the local scan, so
   * one key means "show me what is actually there now".
   * @param {KeyboardEvent} e
   */
  function onRefreshKey(e) {
    if (!open) return;
    if (!(e.ctrlKey || e.metaKey) || e.altKey) return;
    const key = e.key.toLowerCase();
    /** @param {() => void} run */
    const take = (run) => { e.preventDefault(); e.stopPropagation(); run(); };
    // Mod+Shift+Enter: resume the last connection, the footer's "Resume".
    if (e.shiftKey) {
      if (key === "enter") {
        const last = saved.find((c) => c.id === lastId);
        if (last && !isBusy) take(() => void connectWith(last));
      }
      return;
    }
    // The dialog's own chords, caught in the capture phase for the same reason
    // as ⌘R (see <svelte:window> below): the fields inside stop propagation, and
    // Mod+N / Mod+F / Mod+L would otherwise reach the app or the webview.
    if (key === "r") {
      take(() => {
        saved = loadSavedConnections().sort(byLastConnected);
        void refreshLocal();
      });
    } else if (key === "n") {
      take(() => newConnectionForm());
    } else if (key === "f") {
      take(() => {
        if (!railOpen) { railOpen = true; saveRail(); }
        void tick().then(() => { savedSearchEl?.focus(); savedSearchEl?.select?.(); });
      });
    } else if (key === "l") {
      take(() => {
        // The paste field on the picker; on the form, the "Paste a URL" bar.
        if (step === "pick") {
          quickUriEl?.focus();
          quickUriEl?.select?.();
        } else if (entryMode === "manual" && hasFieldToggle) {
          importOpen = true;
          void tick().then(() => document.getElementById("cn-import-uri")?.focus());
        } else {
          backToPick();
          void tick().then(() => quickUriEl?.focus());
        }
      });
    }
  }

  async function refreshLocal() {
    localPhase = "scanning";
    // Discovery is a convenience on both sides. A failure means "nothing found",
    // never an error banner over a dialog opened to do something else.
    const [s, d, m] = await Promise.all([
      scanLocalStudios().catch(() => []),
      scanDockerDatabases().catch(() => []),
      scanMachineDatabases().catch(() => []),
    ]);
    studios = s;
    dockerDbs = d;
    machineDbs = m;
    localPhase = "done";
  }

  /**
   * One row in "Running on this machine", from either source.
   * @typedef {{
   *   id: string, mark: string, trailingMark: string | null, title: string,
   *   badge: string, subtitle: string, hint: string, conn: any | null,
   * }} LocalTarget
   */

  /** @param {import('$lib/api.js').DetectedStudio} s */
  function studioTarget(s) {
    const built = s.engine ? studioConnection(s) : null;
    return /** @type {LocalTarget} */ ({
      id: studioId(s),
      mark: s.tool,
      trailingMark: s.engine,
      title: s.projectName,
      badge: `:${s.port}`,
      subtitle: built ? s.target : (s.reason ?? ""),
      hint: built
        ? `${s.target} - from ${s.projectDir}/${s.source}`
        : (s.reason ?? ""),
      conn: built && {
        ...built,
        origin: "studio",
        tool: s.tool,
        toolLabel: s.toolLabel,
      },
    });
  }

  /** @param {import('$lib/api.js').DockerDatabase} d */
  function dockerTarget(d) {
    const label = `Docker · ${d.image}`;
    // Engine-shaped extras: the container is on loopback, so TLS is off unless
    // the engine refuses to speak without it (SQL Server does).
    const extras =
      d.engine === "redis"
        ? { db: 0, tls: false }
        : d.engine === "mssql"
          ? { encrypt: true, trustCert: true }
          : d.engine === "clickhouse"
            ? { secure: false }
            : { ssl: false };
    return /** @type {LocalTarget} */ ({
      id: `docker:${d.name}`,
      mark: d.engine,
      trailingMark: null,
      title: d.name,
      badge: d.reason ? "" : `:${d.port}`,
      subtitle: d.reason ?? d.target,
      hint:
        d.reason ??
        `${d.image} · container ${d.containerId} · ${d.user ? d.user + "@" : ""}${d.target}`,
      conn: d.reason
        ? null
        : {
            id: `docker:${d.name}`,
            type: d.engine,
            name: d.name,
            host: d.host,
            port: d.port,
            user: d.user,
            password: d.password,
            database: d.database,
            origin: "docker",
            toolLabel: label,
            ...extras,
          },
    });
  }

  /** @param {import('$lib/api.js').MachineDatabase} m */
  function machineTarget(m) {
    return /** @type {LocalTarget} */ ({
      id: m.id,
      mark: m.engine,
      trailingMark: null,
      title: m.name,
      badge: `:${m.port}`,
      subtitle: m.target,
      hint: `${m.name} (pid ${m.pid}) - connects as ${m.user || "the default user"}`,
      conn: {
        id: m.id,
        type: m.engine,
        name: `${m.name} (:${m.port})`,
        host: m.host,
        port: m.port,
        user: m.user,
        password: "",
        database: m.database,
        origin: "machine",
        toolLabel: "Installed on this machine",
        ...(m.engine === "redis" ? { db: 0, tls: false } : { ssl: false }),
      },
    });
  }

  const matchesQuery = (/** @type {LocalTarget} */ t) => {
    const q = engineQuery.trim().toLowerCase();
    return !q || `${t.title} ${t.subtitle}`.toLowerCase().includes(q);
  };

  // Three sources, three groups, in the order you'd reach for them: the studio
  // you have open right now, then what's installed, then containers. A Docker
  // container's server process is invisible to the machine scan (its socket
  // lives in the container's netns), but a stray port collision would still be
  // a duplicate row, so Docker wins on port.
  const dockerPorts = $derived(
    new Set(dockerDbs.filter((d) => !d.reason).map((d) => d.port)),
  );
  const localGroups = $derived(
    [
      {
        key: "studios",
        label: "Local studios",
        targets: studios.map(studioTarget).filter(matchesQuery),
      },
      {
        key: "machine",
        label: "Installed on this machine",
        targets: machineDbs
          .filter((m) => !dockerPorts.has(m.port))
          .map(machineTarget)
          .filter(matchesQuery),
      },
    ].filter((g) => g.targets.length > 0),
  );
  // Containers sit below the engine picker: eight of them above it pushed the
  // thing most people came here for off the first screen. A container with no
  // published port can't be opened from the host, so it isn't offered at all.
  const dockerTargets = $derived(
    dockerDbs
      .filter((d) => !d.reason)
      .map(dockerTarget)
      .filter(matchesQuery),
  );
  const localMatches = $derived([
    ...localGroups.flatMap((g) => g.targets),
    ...dockerTargets,
  ]);

  /** Stable per project, so query history follows a studio across restarts. */
  function studioId(s) {
    return `studio:${s.tool}:${s.projectDir}`;
  }

  /**
   * A detected studio as a connection payload. Host engines go through the same
   * URI parser as the paste bar, so one parser owns every connection string in
   * the app. Null when the URL didn't parse.
   */
  function studioConnection(s) {
    const base = { id: studioId(s), name: `${s.projectName} · ${s.toolLabel}` };
    if (s.engine === "sqlite")
      return { ...base, type: "sqlite", filePath: s.filePath };
    if (s.engine === "libsql")
      return {
        ...base,
        type: "libsql",
        url: s.url,
        authToken: s.authToken ?? "",
      };
    if (s.engine === "d1")
      return {
        ...base,
        type: "d1",
        accountId: s.accountId,
        databaseId: s.databaseId,
        apiToken: s.apiToken,
      };
    const uriType =
      s.engine === "mysql"
        ? "mysql"
        : s.engine === "mssql"
          ? "mssql"
          : "postgres";
    const parsed = parseConnectionUri(uriType, s.url ?? "");
    if (!parsed || "error" in parsed || !("host" in parsed)) return null;
    return {
      ...base,
      type: s.engine,
      host: parsed.host,
      port: Number(parsed.port),
      database: parsed.database,
      user: parsed.user,
      password: parsed.password,
      ...("encrypt" in parsed
        ? { encrypt: parsed.encrypt, trustCert: parsed.trustCert }
        : { ssl: parsed.ssl }),
    };
  }

  /**
   * Open a local target. Nobody typed these credentials, but the connection
   * still has to survive a disconnect and an app restart - so it joins the list
   * under a stable id and becomes "last connection", and the normal resume path
   * brings it back even once the studio or container scan is stale.
   * @param {LocalTarget} t
   */
  async function connectLocal(t) {
    if (t.conn) await connectWith(t.conn);
  }

  async function handleTest() {
    const myOp = ++opId;
    testing = true;
    error = "";
    testOk = false;
    try {
      // In connection-string mode the payload is built from the individual
      // fields, so parse the URI into them first (finally clears `testing`).
      const p = formPayload();
      if (p.type === "sqlite") await testSqliteConnection(p);
      else if (p.type === "d1") await testD1Connection(p);
      else if (p.type === "libsql") await testLibSqlConnection(p);
      else if (p.type === "mysql" || p.type === "mariadb")
        await testMysqlConnection(p);
      else if (p.type === "clickhouse") await testClickhouseConnection(p);
      else if (p.type === "duckdb") await testDuckdbConnection(p);
      else if (p.type === "mssql") await testMssqlConnection(p);
      else if (p.type === "redis") await testRedis(p);
      else await testPostgresConnection(p);
      if (myOp !== opId) return; // cancelled by the user
      testOk = true;
      // Success is a toast for the same reason failure is: the answer belongs
      // next to the button you pressed, not in a chip you have to go find.
      toast.success("Connection OK", { description: statusTarget });
    } catch (e) {
      if (myOp === opId) failWith(friendlyError(e));
    } finally {
      if (myOp === opId) testing = false;
    }
  }

  // Engines addressed by host + port, with the port a blank field saves as.
  /** Shape a form payload into the persisted SavedConnection row (Save and Connect share it). */
  function buildSavedConn(payload, id, lastConnectedAt) {
    const defaultPort = defaultPortFor(payload.type);
    return {
      id,
      ...payload,
      port:
        defaultPort !== undefined
          ? Number(payload.port) || defaultPort
          : undefined,
      lastConnectedAt,
      readOnly: readOnly || undefined,
    };
  }

  /**
   * Persist the form WITHOUT connecting. Same validation and payload as
   * handleConnect, minus the connect round trip and the dialog close - so a
   * connection can be filed away (or an edit corrected) while the current
   * session stays live. Re-stamps `baseline` so the "Unsaved" dot clears and the
   * close guard stops treating the form as dirty.
   */
  function handleSave() {
    if (!editingId && saved.length >= maxConnections) {
      failWith(
        `Free plan allows ${maxConnections} saved connections. Upgrade to Stroke Pro for unlimited.`,
      );
      return;
    }
    error = "";
    const payload = formPayload();
    if (blockedAsDuplicate(payload, handleSave)) return;
    const existing = editingId ? saved.find((s) => s.id === editingId) : null;
    const id = existing?.id ?? newConnectionId();
    // lastConnectedAt is deliberately NOT touched - nothing was connected, so
    // this must not jump to the top of the recents list or become "Resume".
    saved = upsertConnection(
      buildSavedConn(payload, id, existing?.lastConnectedAt),
    ).sort(byLastConnected);
    // Keep editing the row we just saved, so a follow-up Connect updates it
    // instead of creating a duplicate.
    if (lastPersistFailed()) {
      failWith(
        "Couldn't save this connection - local storage is full. Delete a few saved queries or connections and try again.",
      );
      return;
    }
    editingId = id;
    baseline = snapshot();
    justSaved = true;
    clearTimeout(justSavedTimer);
    justSavedTimer = setTimeout(() => {
      justSaved = false;
    }, 2000);
  }

  /**
   * Dial the form's connection.
   *
   * `save: false` is the plain Connect: it opens the session and writes nothing,
   * for the case where the row is already on file (or deliberately is not).
   * @param {{ save?: boolean }} [opts]
   */
  async function handleConnect(opts = {}) {
    const save = opts.save !== false;
    if (save && !editingId && saved.length >= maxConnections) {
      failWith(
        `Free plan allows ${maxConnections} saved connections. Upgrade to Stroke Pro for unlimited.`,
      );
      return;
    }
    // Built before the spinner goes up: the duplicate check needs the resolved
    // target, and in connection-string mode that is what parses the URI into
    // fields. It reads state and allocates an object - nothing that can fail.
    const payload = formPayload();
    // Only for a row that does not exist yet: connecting to a connection you
    // already have saved is not a filing decision, and being asked about it on
    // the way in is the wrong question at the wrong time.
    if (save && !editingId && blockedAsDuplicate(payload, () => void handleConnect({ save }))) return;
    const myOp = ++opId;
    connecting = editingId ?? "__new__";
    error = "";
    try {
      const existing = editingId ? saved.find((s) => s.id === editingId) : null;
      const id = existing?.id ?? newConnectionId();
      // Persist BEFORE dialling. Everything the user typed is worth keeping the
      // moment they commit to it, and a connect can fail for reasons that have
      // nothing to do with what they entered - no network, VPN down, server
      // asleep. Saving only on success meant a dropped connection threw the
      // whole form away and they had to retype it, which is what made
      // connections look like they were disappearing after being saved.
      // lastConnectedAt stays at its previous value so a failed attempt does not
      // jump the row to the top of the recents list or turn it into "Resume".
      if (save) {
        saved = upsertConnection(
          buildSavedConn(payload, id, existing?.lastConnectedAt),
        ).sort(byLastConnected);
        editingId = id;
      }
      await openConnection(payload);
      if (myOp !== opId) return; // cancelled by the user
      const saved_conn = buildSavedConn(payload, id, Date.now());
      if (save) {
        saved = upsertConnection(saved_conn).sort(byLastConnected);
        setLastConnectionId(id);
      }
      open = false;
      // The id is what files this as the last connection - and the shell writes
      // the payload back under it, so an unsaved EDIT must not travel with one
      // or "connect without saving" would save. An untouched saved row has
      // nothing to leak, so it keeps its id and still resumes on next launch.
      const handoverId = save || (editingId && !isDirty) ? id : "";
      await onconnected(saved_conn, handoverId);
    } catch (e) {
      if (myOp === opId) failWith(friendlyError(e));
    } finally {
      if (myOp === opId) connecting = null;
    }
  }

  const isBusy = $derived(testing || !!connecting);

  /**
   * A second connection to a database you already have saved is usually a
   * mistake - a re-paste of the same URI, or a second pass through the Docker
   * scan - and the list is sorted by last use, so the copy hides the original.
   * It is not always a mistake though (a read-only twin, a different name for a
   * different project), so this asks rather than refuses.
   * @type {{ existing: any, proceed: () => void } | null}
   */
  let duplicatePrompt = $state(null);
  /** Set by "Save anyway", so the retry of the same action goes straight through. */
  let duplicateAck = false;

  /**
   * Returns true when the action should stop and let the user answer first.
   * @param {any} payload @param {() => void} proceed
   */
  function blockedAsDuplicate(payload, proceed) {
    if (duplicateAck) return false;
    const existing = findDuplicateConnection(payload, saved, editingId);
    if (!existing) return false;
    duplicatePrompt = { existing, proceed };
    return true;
  }

  // Transient "Saved" confirmation on the Save button (no toast - the dialog
  // stays open, so the feedback belongs on the control that was pressed).
  let justSaved = $state(false);
  /** @type {ReturnType<typeof setTimeout> | undefined} */
  let justSavedTimer;

  // ── Dirty tracking + close guard ─────────────────────────────────────────────
  // A snapshot of every editable field; `baseline` is re-stamped each time a form
  // loads (resetForm), so isDirty flips true the moment the user changes anything.
  let baseline = $state("");
  let confirmDiscardOpen = $state(false);
  function snapshot() {
    return JSON.stringify([
      dbType,
      name,
      host,
      port,
      database,
      user,
      password,
      ssl,
      secure,
      encrypt,
      trustCert,
      filePath,
      accountId,
      databaseId,
      apiToken,
      libsqlUrl,
      libsqlToken,
      readOnly,
      sshEnabled,
      sshHost,
      sshPort,
      sshUsername,
      sshKeyPath,
      sshAuth,
      sshKeepalive,
      sslMode,
      sslCaPath,
    ]);
  }
  const isDirty = $derived(snapshot() !== baseline);

  /** Full-width status-bar target for the current form. */
  const statusTarget = $derived.by(() => {
    if (["sqlite", "sqlite-memory", "duckdb", "duckdb-memory"].includes(dbType))
      return filePath || ":memory:";
    if (dbType === "libsql") return libsqlUrl || "—";
    if (dbType === "d1") return databaseId ? `${databaseId.slice(0, 8)}…` : "—";
    return `${host || "—"}:${port || "—"}/${database || ""}`;
  });

  /** Attempt to close the dialog - guard against discarding unsaved edits. */
  function requestClose() {
    if (isBusy) return;
    if (isDirty) {
      confirmDiscardOpen = true;
      return;
    }
    open = false;
  }
  function discardAndClose() {
    confirmDiscardOpen = false;
    baseline = snapshot(); // stop isDirty re-firing during the close animation
    open = false;
  }

  // Field labels are sentence case, not uppercase micro-labels: a form with
  // eight of those stacked reads as shouting, and DESIGN_SYSTEM reserves the
  // uppercase treatment for section headings (§10).
  const lbl = "mb-1.5 block text-ui-xs font-medium text-foreground/75";
  // Segmented pill switch (entry-mode + field-mode) - shared base for consistency.
  // A quiet segmented pair, right-aligned beside its section heading - not the
  // full-width bordered block it used to be, which competed with the fields it
  // was only there to switch between.
  // Every field row uses this one 6-column template. Rows that each invented
  // their own split (1fr+110px here, 50/50 there) meant no two column edges in
  // the form lined up, which is most of what made it look thrown together.
  const row6 = "grid grid-cols-6 gap-x-4";
  const inp =
    "field-surface h-9 w-full bg-transparent px-3 text-ui-xs text-foreground placeholder:text-muted-foreground placeholder:font-normal outline-none";
  // Ports, keepalives and the like: figures in a narrow field, where a
  // proportional 1 makes a three-digit value look off-centre.
  const inpNum =
    inp +
    " [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none";

  /**
   * Pick a file for one of the transport paths.
   *
   * Typing a path into a text field is how you find out you typed it wrong at
   * connect time. The field stays editable - a path from a password manager or
   * a CI secret gets pasted, not browsed to.
   * @param {'ca' | 'key'} which
   */
  async function pickTransportFile(which) {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const path = await open(
        which === "ca"
          ? {
              title: "Select CA certificate",
              filters: [
                { name: "Certificates", extensions: ["pem", "crt", "cer", "ca"] },
                { name: "All files", extensions: ["*"] },
              ],
            }
          : {
              title: "Select private key",
              filters: [{ name: "All files", extensions: ["*"] }],
            },
      );
      if (typeof path !== "string" || !path) return;
      if (which === "ca") sslCaPath = path;
      else sshKeyPath = path;
    } catch {
      /* browser/non-Tauri env - the field is still typeable */
    }
  }

  async function pickSqliteFile() {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const path = await open({
        title: "Select SQLite database",
        filters: [
          { name: "SQLite", extensions: ["db", "sqlite", "sqlite3"] },
          { name: "All files", extensions: ["*"] },
        ],
      });
      if (typeof path === "string" && path) filePath = path;
    } catch {
      /* browser/non-Tauri env */
    }
  }

  async function pickDuckdbFile() {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const path = await open({
        title: "Select DuckDB database",
        filters: [
          { name: "DuckDB", extensions: ["duckdb", "ddb", "db"] },
          { name: "All files", extensions: ["*"] },
        ],
      });
      if (typeof path === "string" && path) filePath = path;
    } catch {
      /* browser/non-Tauri env */
    }
  }
</script>

<!-- ⌘R / Ctrl+R rescans what's running locally while the dialog is open. The
     preventDefault matters: without it the webview reloads the whole app. -->
<!-- ⌘R is registered in the CAPTURE phase, deliberately.
     `svelte:window onkeydown` listens on the bubble, so any handler between the
     focused element and window that calls stopPropagation swallows it - and the
     dialog is full of inputs and a bits-ui overlay that do exactly that. Capture
     runs before any of them, which is also the only way to be sure preventDefault
     lands before the webview treats ⌘R as "reload the app". -->
<svelte:window
  onkeydowncapture={onRefreshKey}
  onkeydown={(e) => {
    if (!open) return;
    if (
      (e.ctrlKey || e.metaKey) &&
      !e.altKey &&
      !e.shiftKey &&
      e.key.toLowerCase() === "b"
    ) {
      e.preventDefault();
      railOpen = !railOpen;
      saveRail();
    }
  }}
/>

<!-- SSH Tunnel section (shared by PG and MySQL forms) -->
<!-- Advanced options, SSL / SSH tunnel / encryption / read-only. Laid out to
     fill the available width (toggle row + horizontal SSH grid); wraps to a
     column on the narrow provider/D1 collapsible. -->
<!-- Transport: the two things that sit between this app and the database, each
     in a panel of its own, plus the one switch that changes what the session is
     allowed to do.

     They used to be three checkboxes in a six-column row with the SSH fields
     spilling out underneath: "Connect via SSH tunnel" wrapped onto two lines at
     most widths, turning it on grew four fields into the middle of the form, and
     TLS was one boolean with no way to say what it should verify. A disclosure
     panel per concern keeps the form the same height until you need one, and
     gives each its own fields, its own note and its own switch. -->
{#snippet transportPanel(
  /** @type {string} */ id,
  /** @type {string} */ title,
  /** @type {string} */ summary,
  /** @type {string} */ icon,
  /** @type {boolean} */ enabled,
  /** @type {(v: boolean) => void} */ onToggle,
  /** @type {boolean} */ isOpen,
  /** @type {() => void} */ onDisclose,
  /** @type {import('svelte').Snippet} */ body,
)}
  <!-- `rounded-xl` (12px), not `rounded-lg`: the fields inside carry the 10px
       `--radius-field`, and an outer radius smaller than its contents is the
       thing that reads as "off" without anyone being able to say why. -->
  <section class="overflow-hidden rounded-xl border border-border/50 bg-card/30">
    <!-- Two controls, not one: the row discloses, the switch enables. Nesting a
         switch inside the disclosure button would make one hit area that does
         two things and announce as neither. -->
    <!-- A two-line header needs room for two lines. At h-10 with the inherited
         1.5 leading, the title and summary came to 33px of the 40px row: 3px of
         air top and bottom, the two lines touching each other, and the chevron
         10px off the card's own border. h-14 with `leading-tight` on both lines
         puts 12px above and below the pair and 2px between them, which is what
         separates a title from its caption rather than stacking them. -->
    <!-- One leading edge for the whole card: the icon well, then the title, and
         the body below indented to that same title edge. It used to lead with a
         chevron AND an icon, and the body started back at the card's padding,
         so the fields lined up with neither. The chevron moved to the trailing
         end, where a disclosure affordance is expected. The hover covers the
         whole header row, not the part left of the switch. -->
    <div class="flex items-center gap-3 pe-3.5 transition-colors has-[>button:hover]:bg-muted/25">
      <button
        type="button"
        aria-expanded={isOpen}
        aria-controls="{id}-body"
        onclick={onDisclose}
        class="flex h-14 min-w-0 flex-1 items-center gap-3 ps-3.5 text-left outline-none focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring"
      >
        <span class="flex size-8 shrink-0 items-center justify-center rounded-lg bg-muted/50" aria-hidden="true">
          <Icon name={icon} class="size-4 text-muted-foreground" />
        </span>
        <span class="min-w-0 flex-1">
          <span class="block truncate text-ui-sm font-medium leading-tight text-foreground">{title}</span>
          <span class="mt-0.5 block truncate text-ui-2xs leading-tight text-muted-foreground">{summary}</span>
        </span>
        <Icon
          name="chevron-down"
          class={cn(
            "size-4 shrink-0 text-muted-foreground transition-transform duration-150 ease-out",
            isOpen && "rotate-180",
          )}
          aria-hidden="true"
        />
      </button>
      <span class="h-5 w-px shrink-0 bg-border/50" aria-hidden="true"></span>
      <label class="flex shrink-0 cursor-pointer select-none items-center gap-2 py-2">
        <span class="text-ui-3xs text-muted-foreground">{enabled ? "On" : "Off"}</span>
        <!-- The role announces checked/unchecked, so the name must not say it
             again; the visible On/Off text is the redundant cue for everyone
             who is not hearing it. -->
        <Checkbox
          id="{id}-enabled"
          checked={enabled}
          aria-label={title}
          onCheckedChange={(v) => onToggle(v === true)}
        />
      </label>
    </div>
    {#if isOpen}
      <!-- `inert` when off, not merely dimmed: a field you can focus and type
           into that will not be used is worse than no field. The switch stays
           reachable because it is outside this element. -->
      <div
        id="{id}-body"
        inert={!enabled || undefined}
        class={cn(
          // ps-14.5 = header ps-3.5 + the 32px well + gap-3: the title's edge.
          "flex flex-col gap-3.5 border-t border-border/40 py-3.5 ps-14.5 pe-3.5 transition-opacity duration-150 ease-out",
          !enabled && "opacity-45",
        )}
      >
        {@render body()}
      </div>
    {/if}
  </section>
{/snippet}

<!-- A note inside a panel: what the option does to the connection, in one
     sentence, where the decision is being made. -->
{#snippet panelNote(/** @type {string} */ text)}
  <!-- Plain text, not a filled box: a surface inside the card's surface read as
       a third layer, and its own padding pulled the sentence off the edge the
       fields below it start on. -->
  <p class="max-w-[72ch] text-pretty text-ui-2xs leading-relaxed text-muted-foreground">{text}</p>
{/snippet}

<!-- A path field with a Browse button. The field stays typeable: a path often
     arrives pasted from a password manager or a CI secret. -->
{#snippet pathField(
  /** @type {string} */ id,
  /** @type {string} */ label,
  /** @type {string} */ value,
  /** @type {string} */ placeholder,
  /** @type {(v: string) => void} */ onInput,
  /** @type {() => void} */ onBrowse,
  /** @type {string} */ hint = "",
)}
  <div class="min-w-0">
    <label for={id} class={lbl}>{label}</label>
    <div class="flex min-w-0 gap-1.5">
      <Input
        {id}
        {placeholder}
        value={value}
        spellcheck="false"
        oninput={(e) => onInput(e.currentTarget.value)}
        aria-describedby={hint ? `${id}-hint` : undefined}
        class={cn(inp, "font-mono text-ui-2xs")}
      />
      <button
        type="button"
        onclick={onBrowse}
        class="field-surface inline-flex h-8 shrink-0 items-center px-2.5 text-ui-2xs text-muted-foreground transition-[background-color,color,scale] duration-150 ease-out hover:bg-accent hover:text-foreground active:scale-[0.96]"
      >
        Browse…
      </button>
    </div>
    {#if hint}
      <p id="{id}-hint" class="mt-1 text-ui-3xs leading-snug text-muted-foreground">{hint}</p>
    {/if}
  </div>
{/snippet}

{#snippet advancedFields()}
  {@const isPgMy =
    dbType === "postgres" ||
    dbType === "cockroachdb" ||
    dbType === "mysql" ||
    dbType === "mariadb"}
  {@const isMy = dbType === "mysql" || dbType === "mariadb"}
  <!-- 12px between two bordered cards, not 10: the fields inside one card sit
       14px apart, and a gap between cards tighter than the gap within one makes
       the pair read as a single block. -->
  <div class="flex flex-col gap-3">
    {#if isPgMy}
      <!-- ── TLS ─────────────────────────────────────────────────────────── -->
      {#snippet sslBody()}
        {@render panelNote(
          "Require encrypts the connection but does not check the server's certificate. The verifying levels do check it, against a CA - name one below unless this machine already trusts it.",
        )}
        <div class="grid grid-cols-1 gap-3.5 sm:grid-cols-2">
          <div class="min-w-0 max-w-[16rem]">
            <span id="cn-ssl-mode-label" class={lbl}>Verification</span>
            <SearchableMenu
              items={SSL_MODES}
              searchThreshold={99}
              contentClass="w-[18rem]"
              align="start"
              onselect={(it) => (sslMode = it.value)}
            >
              {#snippet trigger(props)}
                <button
                  {...props}
                  type="button"
                  aria-labelledby="cn-ssl-mode-label"
                  class="field-surface flex h-8 w-full min-w-0 items-center gap-1.5 px-2.5 text-left text-ui-xs transition-colors hover:bg-accent/40"
                >
                  <span class="min-w-0 flex-1 truncate text-foreground">
                    {SSL_MODES.find((m) => m.value === sslMode)?.label ?? "Require"}
                  </span>
                  <Icon name="chevron-down" class="size-3.5 shrink-0 text-muted-foreground" />
                </button>
              {/snippet}
              {#snippet item(it)}
                <span class="min-w-0 flex-1">
                  <span class="block truncate">{it.label}</span>
                  <span class="block truncate text-ui-3xs text-muted-foreground">{it.desc}</span>
                </span>
                {#if it.value === sslMode}
                  <Icon name="check" class="size-3.5 shrink-0 text-primary" />
                {/if}
              {/snippet}
            </SearchableMenu>
          </div>
          {#if sslMode !== "require"}
            {@render pathField(
              "cn-ssl-ca",
              "CA certificate",
              sslCaPath,
              "~/certs/server-ca.pem",
              (v) => (sslCaPath = v),
              () => void pickTransportFile("ca"),
            )}
          {/if}
        </div>
      {/snippet}
      {@render transportPanel(
        "cn-ssl",
        "SSL / TLS",
        ssl ? `Encrypted · ${SSL_MODES.find((m) => m.value === sslMode)?.label ?? "Require"}` : "Not encrypted",
        "lock",
        ssl,
        (v) => {
          ssl = v;
          if (v) sslPanelOpen = true;
        },
        sslPanelOpen,
        () => (sslPanelOpen = !sslPanelOpen),
        sslBody,
      )}

      <!-- ── SSH tunnel ──────────────────────────────────────────────────── -->
      {#snippet sshBody()}
        <!-- One note for the panel, rather than a line of helper text under two
             of the six fields: those hints set the row's height, wrapped at the
             narrow columns they sat in, and left the grid looking like two rows
             that had nothing to do with each other. -->
        {@render panelNote(
          "Forwarded with your system ssh, so the server needs AllowTcpForwarding, and an encrypted key needs its passphrase in your agent. Keepalive is in seconds; 0 turns it off.",
        )}
        <!-- Two rows at this width: address on the first, credentials on the
             second. It was four stacked rows, which is where the form's height
             came from. -->
        <div class={cn(row6, "items-start gap-y-3")}>
          <div class="col-span-6 min-w-0 sm:col-span-3">
            <label for="cn-ssh-host" class={lbl}>SSH host</label>
            <Input
              id="cn-ssh-host"
              bind:value={sshHost}
              placeholder="bastion.example.com"
              spellcheck="false"
              autocomplete="off"
              class={inp}
            />
          </div>
          <div class="col-span-2 min-w-0 sm:col-span-1">
            <label for="cn-ssh-port" class={lbl}>Port</label>
            <Input
              id="cn-ssh-port"
              bind:value={sshPort}
              type="text"
              inputmode="numeric"
              placeholder="22"
              class={inpNum}
            />
          </div>
          <div class="col-span-4 min-w-0 sm:col-span-2">
            <label for="cn-ssh-user" class={lbl}>SSH username</label>
            <Input
              id="cn-ssh-user"
              bind:value={sshUsername}
              placeholder="ec2-user"
              spellcheck="false"
              autocomplete="off"
              class={inp}
            />
          </div>

          <div class="col-span-6 min-w-0 sm:col-span-2">
            <span id="cn-ssh-auth-label" class={lbl}>Authentication</span>
            <SearchableMenu
              items={SSH_AUTH_MODES}
              searchThreshold={99}
              contentClass="w-[18rem]"
              align="start"
              onselect={(it) => (sshAuth = it.value)}
            >
              {#snippet trigger(props)}
                <button
                  {...props}
                  type="button"
                  aria-labelledby="cn-ssh-auth-label"
                  class="field-surface flex h-8 w-full min-w-0 items-center gap-1.5 px-2.5 text-left text-ui-xs transition-colors hover:bg-accent/40"
                >
                  <span class="min-w-0 flex-1 truncate text-foreground">
                    {SSH_AUTH_MODES.find((m) => m.value === sshAuth)?.label ?? "Key file"}
                  </span>
                  <Icon name="chevron-down" class="size-3.5 shrink-0 text-muted-foreground" />
                </button>
              {/snippet}
              {#snippet item(it)}
                <span class="min-w-0 flex-1">
                  <span class="block truncate">{it.label}</span>
                  <span class="block truncate text-ui-3xs text-muted-foreground">{it.desc}</span>
                </span>
                {#if it.value === sshAuth}
                  <Icon name="check" class="size-3.5 shrink-0 text-primary" />
                {/if}
              {/snippet}
            </SearchableMenu>
          </div>
          {#if sshAuth === "key"}
            <div class="col-span-4 min-w-0 sm:col-span-3">
              {@render pathField(
                "cn-ssh-key",
                "Identity file",
                sshKeyPath,
                "~/.ssh/id_rsa",
                (v) => (sshKeyPath = v),
                () => void pickTransportFile("key"),
              )}
            </div>
          {/if}
          <div class="col-span-2 min-w-0 sm:col-span-1">
            <label for="cn-ssh-keepalive" class={lbl}>Keepalive</label>
            <Input
              id="cn-ssh-keepalive"
              bind:value={sshKeepalive}
              type="text"
              inputmode="numeric"
              placeholder="30"
              class={inpNum}
            />
          </div>
        </div>
      {/snippet}
      {@render transportPanel(
        "cn-ssh",
        "SSH tunnel",
        sshEnabled ? sshHost.trim() || "No host yet" : "Direct connection",
        "terminal",
        sshEnabled,
        (v) => {
          sshEnabled = v;
          if (v) sshPanelOpen = true;
        },
        sshPanelOpen,
        () => (sshPanelOpen = !sshPanelOpen),
        sshBody,
      )}
    {:else if dbType === "clickhouse"}
      <label class="flex cursor-pointer select-none items-center gap-2">
        <Checkbox
          id="cn-ch-secure"
          checked={secure}
          onCheckedChange={(v) => {
            secure = v === true;
            if (secure && port === "8123") port = "8443";
            else if (!secure && port === "8443") port = "8123";
          }}
        />
        <span class="text-ui-xs text-foreground/75">Use HTTPS (TLS)</span>
      </label>
    {/if}

    <!-- SQL Server carries its own TLS pair rather than a `sslmode`. -->
    {#if dbType === "mssql"}
      <div class="flex flex-col gap-3 rounded-xl border border-border/50 bg-card/30 p-3">
        <label class="flex cursor-pointer select-none items-start gap-2">
          <Checkbox
            id="cn-mssql-encrypt"
            checked={encrypt}
            onCheckedChange={(v) => (encrypt = v === true)}
            class="mt-px"
          />
          <span class="flex flex-col">
            <span class="text-ui-xs text-foreground/75">Encrypt connection (TLS)</span>
            <span class="text-ui-3xs leading-snug text-muted-foreground"
              >Encrypts traffic between Stroke and the server.</span
            >
          </span>
        </label>
        <label
          class={cn(
            "flex select-none items-start gap-2",
            encrypt ? "cursor-pointer" : "cursor-not-allowed opacity-45",
          )}
        >
          <Checkbox
            id="cn-mssql-trust"
            checked={trustCert}
            disabled={!encrypt}
            onCheckedChange={(v) => (trustCert = v === true)}
            class="mt-px"
          />
          <span class="flex flex-col">
            <span class="text-ui-xs text-foreground/75">Trust server certificate</span>
            <span class="text-ui-3xs leading-snug text-muted-foreground"
              >Accept self-signed or otherwise untrusted certificates. Needed for many local
              and dev servers.</span
            >
          </span>
        </label>
      </div>
    {/if}

    <!-- Read-only is a property of the session, not of the transport, so it sits
         outside both panels rather than being a third checkbox in a row. -->
    <label class="flex cursor-pointer select-none items-center gap-2 px-0.5 py-1">
      <Checkbox
        id="cn-readonly"
        checked={readOnly}
        onCheckedChange={(v) => (readOnly = v === true)}
      />
      <span class="flex min-w-0 items-center gap-1.5 text-ui-xs text-foreground/75">
        <Icon name="lock" class="size-3.5 shrink-0" aria-hidden="true" />
        Open in read-only mode
      </span>
    </label>
  </div>
{/snippet}

<!-- Read-only for the provider / D1 flows. It used to sit alone inside a
     collapsed "Advanced" disclosure: one option behind a click, plus a second
     divider stacked under the sign-in block's own rule. As a row it shares the
     sign-in block's 32px icon well and 12px gap, so both line up on one edge. -->
{#snippet readOnlyRow()}
  <label for="cn-readonly" class="flex cursor-pointer select-none items-center gap-3">
    <span class="flex size-8 shrink-0 items-center justify-center rounded-lg bg-muted/50" aria-hidden="true">
      <Icon name="lock" class="size-4 text-muted-foreground" />
    </span>
    <span class="flex min-w-0 flex-1 flex-col">
      <span class="text-ui-sm font-medium leading-snug text-foreground">Read-only</span>
      <span id="cn-readonly-hint" class="text-ui-xs leading-relaxed text-muted-foreground">
        Browse and query without changing any data.
      </span>
    </span>
    <Checkbox
      id="cn-readonly"
      aria-describedby="cn-readonly-hint"
      checked={readOnly}
      onCheckedChange={(v) => (readOnly = v === true)}
    />
  </label>
{/snippet}

<DialogPrimitive.Root bind:open>
  <DialogPrimitive.Portal>
    <DialogPrimitive.Overlay
      style="top: var(--app-titlebar-h, 38px); bottom: var(--app-statusbar-h, 0px);"
      class="fixed inset-x-0 z-50 bg-black/65 data-open:animate-in data-closed:animate-out data-open:fade-in-0 data-closed:fade-out-0 ease-out duration-150"
    />
    <DialogPrimitive.Content
      data-connection-modal
      style="top: var(--app-titlebar-h, 38px); bottom: var(--app-statusbar-h, 0px);"
      class="fixed inset-x-0 z-50 flex bg-background text-foreground outline-none data-open:animate-in data-closed:animate-out data-open:fade-in-0 data-closed:fade-out-0 data-open:zoom-in-[0.98] data-closed:zoom-out-[0.98] ease-out duration-200"
      onEscapeKeydown={(e) => {
        if (isDirty && !isBusy) {
          e.preventDefault();
          confirmDiscardOpen = true;
        }
      }}
      onFocusOutside={(e) => {
        // Never let focus leaving the content dismiss the modal. Starting a native
        // window drag/resize from the titlebar makes the webview lose focus, which
        // bits-ui treats as focus-outside and closes the dialog. The modal is only
        // meant to close via ×, Escape, or a genuine outside pointer interaction
        // (handled below) - not because the OS took focus for a window drag.
        e.preventDefault();
      }}
      onInteractOutside={(e) => {
        // Nothing outside the content dismisses this dialog. It is inset to leave
        // the titlebar and status bar usable, so "outside" is window chrome:
        // dragging the window to move or resize it, or a stray click on the tab
        // bar, would otherwise throw away a half-filled connection form. Closing
        // is deliberate only - the × button, Escape, or a successful connect.
        e.preventDefault();
      }}
    >
      <DialogPrimitive.Title class="sr-only">Connections</DialogPrimitive.Title>
      <div
        class="grid h-full w-full min-h-0 grid-rows-[minmax(0,1fr)] overflow-hidden"
        style="grid-template-columns: {railOpen
          ? railWidth
          : 44}px minmax(0, 1fr)"
      >
        <!-- ── Sidebar ─────────────────────────────────────────────── -->
        {#if !railOpen}
          <aside
            class="flex min-h-0 flex-col items-center border-r border-border/15 bg-muted/[0.015] pt-2.5"
          >
            <button
              type="button"
              onclick={() => {
                railOpen = true;
                saveRail();
              }}
              title="Show connections (⌘B)"
              aria-label="Show connections"
              class="inline-flex size-8 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted/50 hover:text-foreground"
            >
              <Icon name="chevrons-right" class="size-4" />
            </button>
          </aside>
        {:else}
          <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
          <aside
            class="relative flex min-h-0 flex-col overflow-hidden border-r border-border/15 bg-muted/[0.015]"
            onkeydowncapture={(e) => {
              // One rule for the whole rail: Tab goes to the connections.
              //
              // Per-control handlers kept missing a stop - the filter, its ✕, the
              // scroll box Chromium makes focusable on its own, a context-menu
              // wrapper - and each miss reads as "Tab did nothing", because none
              // of those stops draw anything. Caught in the capture phase from
              // the rail itself, so it does not matter which of them holds focus.
              // A row is the exception: Tab off a row leaves for the form, which
              // is where it should go next.
              if (e.key !== "Tab" || e.shiftKey || !savedMatches.length) return;
              const el = /** @type {HTMLElement | null} */ (e.target);
              if (el?.closest?.("[data-conn-row]")) return;
              e.preventDefault();
              focusFirstSavedRow();
            }}
          >
            <!-- Title -->
            <div class="flex h-[52px] shrink-0 items-center gap-2 px-4">
              <h2 class="text-ui-sm font-semibold text-foreground">
                Connections
              </h2>
              {#if saved.length > 0}
                <span
                  class="rounded-full bg-muted/60 px-1.5 py-px text-ui-2xs font-medium tabular-nums text-muted-foreground"
                  >{savedQuery ? `${savedMatches.length}/${saved.length}` : saved.length}</span
                >
              {/if}
              <button
                type="button"
                onclick={() => {
                  railOpen = false;
                  saveRail();
                }}
                title="Hide connections (⌘B)"
                aria-label="Hide connections"
                class="ml-auto inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted/50 hover:text-foreground"
              >
                <Icon name="chevrons-left" class="size-4" />
              </button>
            </div>

            <!-- New connection button -->
            <div class="shrink-0 border-t border-border/15 px-2 pb-2 pt-2">
              <!-- Same geometry as a connection row - icon slot, gap and radius -
                   so "New connection" and every saved name start on one left
                   edge. It sat on its own grid before, which read as a control
                   bolted above the list rather than the first entry in it. -->
              <button
                type="button"
                onclick={() => resetForm(null)}
                onkeydown={(e) => {
                  // Straight into the list. Chromium makes a scrollable container
                  // a tab stop of its own (keyboard-focusable scrollers), so left
                  // to the browser this Tab can land on the scroll box instead of
                  // a row - a stop with nothing on it.
                  if (e.key === "Tab" && !e.shiftKey && savedMatches.length) {
                    e.preventDefault();
                    focusFirstSavedRow();
                  }
                }}
                class={cn(
                  "flex w-full items-center gap-2.5 rounded-md border px-2 py-1.5 text-left text-ui-xs outline-none transition-[color,background-color,border-color,transform] duration-150 ease-out focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-ring active:scale-[0.98]",
                  !editingId
                    ? "border-border/40 bg-muted/40 font-medium text-foreground"
                    : "border-transparent text-muted-foreground hover:bg-muted/25 hover:text-foreground",
                )}
              >
                <span class="flex size-6 shrink-0 items-center justify-center">
                  <Icon name="plus" class="size-3.5" />
                </span>
                New connection
              </button>
            </div>

            <!-- Search. Four connections fit on screen; forty do not, and the
                 rail is the fastest way in for someone who already knows which
                 database they want.
                 Shown from two connections up - the point at which there is
                 something to tell apart. It used to appear only past five, so
                 the control materialised out of nowhere as the list grew, and
                 the people most likely to reach for it were the ones who had
                 never seen it. The engine panel opposite always shows its
                 search; this one now matches it. -->
            {#if saved.length > 1}
              <div class="shrink-0 px-2 pb-2">
                <div class="relative">
                  <Icon
                    name="search"
                    class="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground"
                  />
                  <!-- The shared Input, resized to the rail's compact scale, so
                       the focus ring and border states match the engine search
                       rather than being approximated a second time. -->
                  <!-- Not a forward tab stop. The rail's job is the list, and a
                       filter field standing between the two took a whole Tab to
                       pass on the way to it. It is still reachable without a
                       mouse - Shift+Tab off the top row lands here, and typing
                       into it is a click or that chord away - so the path exists,
                       it just is not in the way. -->
                  <Input
                    bind:ref={savedSearchEl}
                    bind:value={savedQuery}
                    placeholder="Filter connections…"
                    aria-label="Filter saved connections"
                    title="Filter connections ({IS_MAC ? '⌘' : 'Ctrl'}+F, or Shift+Tab from the list)"
                    aria-keyshortcuts="Control+F Meta+F"
                    tabindex="-1"
                    autocomplete="off"
                    spellcheck="false"
                    class="h-8 rounded-md border pl-8 pr-8 text-ui-xs"
                    oninput={() => (savedStagger = false)}
                    onkeydown={(e) => {
                      if (e.key === "Escape" && savedQuery) {
                        // Clear first, close the dialog second - Escape on a
                        // non-empty filter should undo the filter, not the dialog.
                        e.preventDefault();
                        e.stopPropagation();
                        savedQuery = "";
                        return;
                      }
                      // Enter takes the top match, the same way clicking it would;
                      // Mod+Enter connects to it instead of just opening it.
                      if (e.key === "Enter" && firstSavedMatch) {
                        e.preventDefault();
                        if (e.metaKey || e.ctrlKey) { if (!connecting) void connectWith(firstSavedMatch); }
                        else resetForm(firstSavedMatch);
                        return;
                      }
                      // Down arrow hands off to the list, so a filtered result can
                      // be reached without going back to the mouse.
                      if (e.key === "ArrowDown" && savedMatches.length) {
                        e.preventDefault();
                        focusFirstSavedRow();
                        return;
                      }
                      // So does Tab. Left to the browser it walks whatever the
                      // scroll container wraps the list in first, which spends a
                      // keystroke on a stop with nothing to show for it.
                      if (e.key === "Tab" && !e.shiftKey && savedMatches.length) {
                        e.preventDefault();
                        focusFirstSavedRow();
                      }
                    }}
                  />
                  {#if savedQuery}
                    <button
                      type="button"
                      aria-label="Clear filter (Escape)"
                      title="Clear filter (Esc)"
                      tabindex="-1"
                      onclick={() => {
                        savedQuery = "";
                        savedSearchEl?.focus();
                      }}
                      class="absolute right-1.5 top-1/2 inline-flex size-6 -translate-y-1/2 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted/60 hover:text-foreground after:absolute after:-inset-1 after:content-['']"
                      ><Icon name="x" class="size-3.5" /></button
                    >
                  {/if}
                </div>
              </div>
            {/if}

            <div class="min-h-0 flex-1 border-t border-border/15 pt-1">
              {#if savedQuery && savedMatches.length === 0}
                <!-- A dead end needs a way out: the filter that emptied the list
                     is the only thing standing between here and the list. -->
                <div class="flex flex-col items-start gap-1.5 px-4 py-3">
                  <p class="text-ui-2xs text-muted-foreground">
                    No connection matches “{savedQuery}”.
                  </p>
                  <button
                    type="button"
                    onclick={() => {
                      savedQuery = "";
                      savedSearchEl?.focus();
                    }}
                    class="rounded text-ui-2xs text-primary transition-colors hover:underline"
                    >Clear filter</button
                  >
                </div>
              {:else if saved.length > 0}
                <ScrollArea type="auto" class="h-full scroll-smooth">
                  <div class="px-2 py-1 flex flex-col gap-0.5">
                    {#each savedMatches as conn, i (conn.id)}
                      {@const isSel = conn.id === editingId}
                      {@const busy2 = connecting === conn.id}
                      {@const cid =
                        conn.filePath === ":memory:" &&
                        (conn.type === "sqlite" || conn.type === "duckdb")
                          ? `${conn.type}-memory`
                          : conn.type}
                      {@const prov = providerOf(conn)}
                      <!-- The provider's mark when there is one (it says where
                           the database lives), else the engine's. The engine is
                           then named in the chip, so neither fact is lost. -->
                      {@const mark = prov ?? cid}
                      <div
                        data-conn-row
                        class={cn(
                          "group relative flex cursor-pointer items-center gap-2.5 rounded-md px-2 py-2 transition-[color,background-color,transform] duration-150 ease-[cubic-bezier(0.23,1,0.32,1)] outline-none focus-visible:bg-muted/60 focus-visible:text-foreground focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring active:scale-[0.98]",
                          savedStagger && "cn-stagger-in",
                          isSel
                            ? "bg-muted/50 text-foreground"
                            : "text-muted-foreground hover:bg-muted/30 hover:text-foreground",
                        )}
                        style={savedStagger
                          ? `animation-delay: ${Math.min(i, 12) * 40}ms`
                          : ""}
                        role="button"
                        tabindex={conn.id === savedRovingId ? 0 : -1}
                        onclick={() => resetForm(conn)}
                        ondblclick={() => { if (!connecting) void connectWith(conn); }}
                        onkeydown={(e) => {
                          // Enter opens the connection in the form, the same as a
                          // click. Mod+Enter connects, the same as a double-click -
                          // the keyboard gets both gestures the mouse has.
                          if (e.key === "Enter" || e.key === " ") {
                            e.preventDefault();
                            if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
                              if (!connecting) void connectWith(conn);
                            } else resetForm(conn);
                            return;
                          }
                          if (e.key === "Delete" || e.key === "Backspace") {
                            e.preventDefault();
                            handleDelete(conn.id, true);
                            return;
                          }
                          // Shift+Tab is the way back to the filter, the mirror of
                          // the Tab that got here.
                          if ((e.key === "Tab" || e.code === "Tab") && e.shiftKey && savedSearchEl) {
                            e.preventDefault();
                            savedSearchEl.focus();
                            return;
                          }
                          // Arrow keys walk the list; Up off the top row returns
                          // to the filter, so the whole rail is one keyboard path.
                          if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
                          e.preventDefault();
                          const rows = [...document.querySelectorAll("[data-conn-row]")];
                          const next = rows.indexOf(e.currentTarget) + (e.key === "ArrowDown" ? 1 : -1);
                          if (next < 0) savedSearchEl?.focus();
                          else /** @type {HTMLElement|undefined} */ (rows[next])?.focus();
                        }}
                        title="Click to edit · double-click to connect ({IS_MAC ? '⌘' : 'Ctrl'}+Enter)"
                      >
                        {#if isSel}
                          <!-- Selection bar. Hidden while the keyboard focus
                               outline is up: the outline is drawn 2px inside the
                               row, so the bar landed right against its left edge
                               and read as a doubled border. -->
                          <span
                            class="absolute inset-y-1.5 left-0 w-0.5 rounded-full bg-foreground/70 transition-opacity duration-150 group-focus-visible:opacity-0"
                          ></span>
                        {/if}
                        <!-- Fixed-size icon slot keeps every row's text left-edge aligned. Fades to Play on hover. -->
                        <button
                          type="button"
                          class="relative flex size-6 shrink-0 items-center justify-center rounded-md disabled:opacity-30"
                          title="Connect ({IS_MAC ? '⌘' : 'Ctrl'}+Enter)"
                          aria-label="Connect to {conn.name || 'this connection'}"
                          tabindex="-1"
                          disabled={!!connecting}
                          onclick={(e) => {
                            e.stopPropagation();
                            void connectWith(conn);
                          }}
                        >
                          <span
                            class="absolute inset-0 flex items-center justify-center transition-opacity duration-150 group-hover:opacity-0"
                          >
                            {#if busy2}
                              <Icon
                                name="loader-2"
                                class="size-4 animate-spin"
                              />
                            {:else}
                              <DbIcon
                                id={mark}
                                class={cn("size-4", engineTint(mark))}
                              />
                            {/if}
                          </span>
                          <span
                            class="absolute inset-0 flex items-center justify-center opacity-0 transition-opacity duration-150 group-hover:opacity-100"
                          >
                            <Icon name="play" class="size-3.5" />
                          </span>
                        </button>

                        <!-- Name + engine · detail subtitle -->
                        <div class="min-w-0 flex-1">
                          <p
                            class="truncate text-ui-xs font-medium leading-tight text-foreground/90"
                          >
                            {conn.name || "Unnamed"}
                          </p>
                          <p
                            class="mt-1 flex min-w-0 items-center gap-1.5 text-ui-2xs leading-tight text-muted-foreground"
                          >
                            <span
                              class="shrink-0 rounded border border-border/60 bg-muted/40 px-1 py-px text-ui-3xs font-medium leading-none text-muted-foreground"
                              >{engineLabel(cid)}</span
                            >
                            <span class="min-w-0 truncate">{connSubtitle(conn)}</span>
                          </p>
                        </div>

                        <!-- Trash: hidden until hover -->
                        <button
                          type="button"
                          class="shrink-0 rounded p-0.5 text-muted-foreground opacity-0 transition-opacity duration-150 hover:text-destructive group-hover:opacity-100"
                          title="Delete (Del)"
                          aria-label="Delete {conn.name || 'this connection'}"
                          tabindex="-1"
                          onclick={(e) => {
                            e.stopPropagation();
                            handleDelete(conn.id);
                          }}><Icon name="trash-2" class="size-3" /></button
                        >
                      </div>
                    {/each}
                  </div>
                </ScrollArea>
              {:else}
                <div class="flex h-full items-center justify-center pb-8">
                  <p class="text-ui-2xs text-muted-foreground">
                    No saved connections
                  </p>
                </div>
              {/if}
            </div>
            <!-- Drag the rail wider when connection names are long. -->
            <div class="absolute inset-y-0 right-0 z-20 w-1">
              <ResizeHandle
                axis="x"
                edge="end"
                onresizestart={() => (railDragStart = railWidth)}
                onresize={(dx) =>
                  (railWidth = Math.min(
                    RAIL_MAX,
                    Math.max(RAIL_MIN, railDragStart + dx),
                  ))}
                onresizeend={saveRail}
              />
            </div>
          </aside>
        {/if}

        <!-- ── Form panel ──────────────────────────────────────────── -->
        <div class="relative flex min-h-0 min-w-0 flex-col">
          <!-- ── Header ──────────────────────────────────────────────────────
             One question per screen. Step 1 asks only what you're connecting to;
             the title becomes that choice in step 2, with the back arrow as the
             way to change it - so there is never a form on screen for a database
             nobody has picked yet. -->
          <!-- The header lines up with whatever is under it: the tile column on
               step 1, the full-width form on step 2. -->
          <div
            class="w-full shrink-0 px-8 pt-5"
          >
            {#if step === "pick"}
              <!-- pe-6 keeps the rescan button clear of the dialog's own close
                 button, which floats at right-8 top-5 over this same corner. -->
              <div class="flex items-center gap-3 pe-10">
                <h2
                  class="text-ui-lg font-semibold tracking-tight text-foreground"
                >
                  Connect a database
                </h2>
                <!-- One indicator: the rescan button's own arrow turns while a scan
                     runs. No label beside it, no icon swap to a different spinner
                     and no width change, so the header never shifts. The live
                     region tells screen readers. -->
                <span class="sr-only" role="status">{localPhase === "scanning" ? "Scanning for local databases" : ""}</span>
                <button
                  type="button"
                  title="Rescan local databases (⌘R)"
                  aria-label={localPhase === "scanning" ? "Scanning local databases" : "Rescan local databases"}
                  disabled={localPhase === "scanning"}
                  onclick={() => {
                    saved = loadSavedConnections().sort(byLastConnected);
                    void refreshLocal();
                  }}
                  class="field-surface inline-flex size-8 shrink-0 items-center justify-center text-muted-foreground transition-[color,background-color,scale] duration-150 ease-out hover:bg-accent hover:text-foreground active:scale-[0.96] disabled:pointer-events-none disabled:text-foreground"
                >
                  <Icon
                    name="refresh-cw"
                    class={cn("size-3.5 shrink-0", localPhase === "scanning" && "animate-spin")}
                    aria-hidden="true"
                  />
                </button>
              </div>
            {:else}
              <!-- pe-14, not pe-6: the URL button now sits at the right edge of
                   this row and the dialog's own close ✕ floats over the same
                   corner (right-4, size-8). -->
              <!-- pe-10 = the 32px close button + the row's own 8px gap, so
                   "Paste a URL" and ✕ sit 8px apart like every other pair in
                   this row. The close button shares the 32px edge (right-8)
                   and the h-8 height of these controls. -->
              <div class="flex items-center gap-2 pe-10">
                <button
                  type="button"
                  onclick={backToPick}
                  title="Back"
                  aria-label="Back"
                  class="-ms-2 inline-flex size-8 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted/50 hover:text-foreground"
                >
                  <Icon name="chevron-left" class="size-4" />
                </button>

                <!-- The engine, as a dropdown. It was a page of seventeen tiles
                     you had to visit before this form existed, and then a static
                     title telling you what you had picked - so changing your
                     mind meant going back a page. It is one field of the
                     connection, so it sits in the form like one. -->
                <SearchableMenu
                  items={engineItems}
                  placeholder="Search databases…"
                  contentClass="w-[22rem]"
                  align="start"
                  onselect={(it) => pickEngine(it.value)}
                >
                  {#snippet trigger(props)}
                    <button
                      {...props}
                      type="button"
                      aria-label="Database engine"
                      class="field-surface flex h-8 min-w-0 max-w-[22rem] items-center gap-2 px-2.5 text-left transition-colors hover:bg-accent/40"
                      title={editingId ? name || activeDriver.label : activeDriver.label}
                    >
                      <DbIcon
                        id={activeDriver.id}
                        class={cn("size-4 shrink-0", engineTint(activeDriver.id))}
                      />
                      <span class="min-w-0 truncate text-ui-sm font-medium text-foreground">
                        {editingId ? name || activeDriver.label : activeDriver.label}
                      </span>
                      <Icon name="chevron-down" class="size-3.5 shrink-0 text-muted-foreground" />
                    </button>
                  {/snippet}
                  {#snippet item(it)}
                    <DbIcon id={it.value} class={cn("size-4 shrink-0", engineTint(it.value))} />
                    <span class="min-w-0 flex-1 truncate">{it.label}</span>
                    <span class="shrink-0 text-ui-3xs text-muted-foreground">{it.group}</span>
                    {#if it.value === dbType}
                      <Icon name="check" class="size-3.5 shrink-0 text-primary" />
                    {/if}
                  {/snippet}
                </SearchableMenu>

                <!-- Opens the URL bar above the footer. It sat down there beside
                     the status text, in the row your eye goes to last, wearing
                     the quietest treatment on the form - so the fastest way to
                     fill this form was also the hardest thing on it to find. Up
                     here it is a control among controls; the bar it opens stays
                     where the fields are. -->
                {#if entryMode === "manual" && hasFieldToggle}
                  <button
                    type="button"
                    aria-expanded={importOpen}
                    aria-controls="cn-url-bar"
                    title="Fill this form from a connection string"
                    onclick={() => {
                      importOpen = !importOpen;
                      uriHint = "";
                      if (importOpen)
                        void tick().then(() =>
                          document.getElementById("cn-import-uri")?.focus(),
                        );
                    }}
                    class={cn(
                      "field-surface ms-auto inline-flex h-8 shrink-0 items-center gap-1.5 px-2.5 text-ui-2xs transition-colors",
                      importOpen
                        ? "bg-accent/60 text-foreground"
                        : "text-muted-foreground hover:bg-accent/40 hover:text-foreground",
                    )}
                  >
                    <Icon name="link-2" class="size-3.5 shrink-0" aria-hidden="true" />
                    Paste a URL
                  </button>
                {/if}
              </div>
            {/if}
          </div>

          <!-- ── Step 1 · what are we connecting to ─────────────────────────
             A grid of marks, grouped the way the drivers are grouped. Names only:
             blurbs turned this into a wall of prose to read before the first
             decision, and the mark is what people actually recognise. -->
          {#if step === "pick"}
            <ScrollArea type="auto" class="min-h-0 flex-1 scroll-smooth">
              <!-- A connection is a form, and a form has a width. Everything
                   here used to span the panel: on a 2000px window the paste bar
                   was 1900px long, the five provider marks sat in an eight-column
                   auto-fill grid with 200px of air between them, and "New
                   connection" put its chevron 1800px from its own label. One
                   42rem column, one leading edge. -->
              <!-- Left-aligned, not centred: the rail is on the left and the
                   header sits above this, so a column centred in the panel
                   starts somewhere neither of them does. One leading edge. -->
              <!-- Full width: the grids below are four columns, so a wide window
                   buys wider tiles and fewer truncated names, not empty margin. -->
              <div class="@container flex w-full flex-col gap-5 px-8 py-5">
                  <!-- ── Paste a connection string ─────────────────────────
                       The fastest path there is, and the one a developer already
                       has in a clipboard from their provider dashboard or a
                       .env. The engine is in the string, so nothing needs to be
                       picked first: `detectConnectionUri` reads the scheme (and
                       the host, for a provider) and the form opens filled in.
                       "Import from URL" used to live inside the form, behind
                       choosing a driver - which is the one step this removes. -->
                  <div class="flex flex-col gap-1.5">
                    <!-- One frame holding the icon, the field and its action. As
                         two controls, focus drew the field's border and the ring
                         just outside it as two stacked lines, and the disabled
                         Continue sat beside it as a grey block. The frame takes
                         the focus outline; the field inside it is frameless. -->
                    <div
                      class="flex h-9 items-center gap-2 rounded-lg border border-border/70 bg-muted/20 ps-3 pe-1 transition-colors hover:border-border focus-within:border-border focus-within:outline-2 focus-within:outline-offset-1 focus-within:outline-ring/70"
                    >
                        <Icon
                          name="link-2"
                          class="size-3.5 shrink-0 text-muted-foreground"
                          aria-hidden="true"
                        />
                        <input
                          bind:this={quickUriEl}
                          bind:value={quickUri}
                          placeholder="Paste a connection string, or a path to a .db file"
                          aria-label="Paste a connection string"
                          spellcheck="false"
                          autocomplete="off"
                          class="no-focus-ring h-full min-w-0 flex-1 bg-transparent font-mono text-ui-2xs text-foreground outline-none placeholder:font-sans placeholder:text-ui-xs placeholder:text-muted-foreground"
                          onclick={() => void prefillFromClipboard()}
                          oninput={(e) => {
                            quickHint = "";
                            // Cleared on purpose: do not hand the same string
                            // back on the next window focus.
                            const v = /** @type {HTMLInputElement} */ (e.currentTarget).value;
                            if (!v.trim() && clipboardFilled) clipboardDismissed = clipboardFilled;
                          }}
                          onkeydown={(e) => {
                            if (e.key !== "Enter") return;
                            e.preventDefault();
                            useQuickUri();
                          }}
                        />
                      {#if !quickUri}
                        <Kbd combo="Mod+L" size="sm" class="shrink-0 text-muted-foreground" />
                      {/if}
                      <!-- Quiet until there is something to continue with, then
                           primary. Disabled it reads as part of the field, not
                           as a broken button next to it. -->
                      <button
                        type="button"
                        disabled={!quickUri.trim()}
                        onclick={useQuickUri}
                        class="inline-flex h-7 shrink-0 items-center gap-1 rounded-md bg-primary px-2.5 text-ui-2xs font-medium text-primary-foreground transition-[background-color,color,scale] duration-150 ease-out hover:bg-primary/90 active:scale-[0.96] disabled:pointer-events-none disabled:bg-transparent disabled:text-muted-foreground"
                      >
                        Continue
                        <Icon name="arrow-right" class="size-3.5 shrink-0" aria-hidden="true" />
                      </button>
                    </div>
                    {#if quickHint}
                      <p class="px-0.5 text-ui-2xs text-muted-foreground">{quickHint}</p>
                    {/if}
                  </div>

                  <!-- ── Providers ─────────────────────────────────────────
                       An account, not a host and port: these four hand back a
                       database list once you are signed in, so they are the only
                       engines worth naming on the front page. -->
                  <div>
                    <h3 class="text-ui-xs font-medium text-foreground/80">
                      Connect with a provider
                    </h3>
                    <!-- Two columns: five providers in an auto-fill track came
                         out as five marks strung across the window with 200px of
                         air between them. -->
                    <!-- Horizontal: five providers on one row once there is
                         room, wrapping to two or three columns only when the
                         panel is narrow. Stacked two-up they took three rows and
                         half the page to say five words. -->
                    <!-- Four columns once there is room, two when narrow. -->
                    <div class="mt-2 grid grid-cols-2 gap-1.5 @2xl:grid-cols-4">
                      {#each PROVIDER_CARDS as id (id)}
                        {@const d = driverById(id)}
                        {@const off = DISABLED_TABS.has(id)}
                        <button
                          type="button"
                          disabled={off}
                          title={off ? `${d.label} - coming soon` : `${d.label} - ${d.desc}`}
                          onclick={() => pickEngine(id)}
                          class={cn(
                            "group flex h-9 items-center gap-2 rounded-lg border px-2.5 text-left outline-none transition-[color,background-color,border-color,scale] duration-150 ease-out",
                            off
                              ? "cursor-not-allowed border-border/40 opacity-45"
                              : "border-border/60 bg-card/40 text-foreground hover:border-border hover:bg-accent/40 focus-visible:border-ring active:scale-[0.98]",
                          )}
                        >
                          <DbIcon
                            id={d.id}
                            class={cn(
                              "size-4 shrink-0 transition-opacity",
                              off ? "opacity-30 grayscale" : engineTint(d.id),
                            )}
                          />
                          <span class="min-w-0 flex-1 truncate text-ui-2xs font-medium">{d.label}</span>
                          {#if off}
                            <span class="shrink-0 text-ui-3xs text-muted-foreground/70">soon</span>
                          {/if}
                        </button>
                      {/each}
                    </div>
                  </div>

                <!-- ── Studios running on this machine ────────────────────────
                   A running `prisma studio` / `drizzle-kit studio` already knows
                   its database, so this offers it directly - one click, no
                   connection string, nothing saved afterwards. -->
                <!-- Only when a group actually has rows: an empty wrapper still
                     carried the column's gap either side of it, which is where
                     the hole between the providers and Docker came from. -->
                {#if localGroups.length > 0}
                  <div class="flex flex-col gap-5">
                    {#each localGroups as group, i (group.key)}
                      {@render localGroup(group.label, group.targets, i === 0)}
                    {/each}
                  </div>
                {/if}

                {#snippet liveDot()}
                  <span class="relative flex size-1.5 shrink-0">
                    <span
                      class="absolute inline-flex size-full animate-ping rounded-full bg-success/60"
                    ></span>
                    <span
                      class="relative inline-flex size-1.5 rounded-full bg-success"
                    ></span>
                  </span>
                {/snippet}

                {#snippet localGroup(
                  /** @type {string} */ label,
                  /** @type {LocalTarget[]} */ targets,
                  /** @type {boolean} */ lead = false,
                  /** @type {string} */ mark = "",
                )}
                  <div>
                    <!-- Sentence case, like the other section headings: small
                         all-caps with wide tracking read as a label on a
                         control rather than the name of a group. The count is
                         a pill so it doesn't read as part of the name. -->
                    <h3 class="flex items-center gap-2 text-ui-xs font-medium text-foreground/80">
                      {#if lead}{@render liveDot()}{/if}
                      {#if mark}<DbIcon id={mark} class={cn("size-4 shrink-0", engineTint(mark))} />{/if}
                      {label}
                      <span
                        class="rounded-full bg-muted/60 px-1.5 py-px font-mono text-ui-3xs tabular-nums text-muted-foreground"
                        aria-label="{targets.length} found">{targets.length}</span
                      >
                    </h3>
                    <!-- A grid of small cards, two or three across. Four
                         containers as full-width two-line rows took as much
                         height as the whole rest of the page; they are the same
                         shape and the same size as each other, so they tile. -->
                    <div class="mt-2 grid grid-cols-1 gap-1.5 @xl:grid-cols-2 @4xl:grid-cols-4">
                      {#each targets as t, i (t.id)}
                        {@const busy = connecting === t.id}
                        <button
                          type="button"
                          disabled={!t.conn || !!connecting}
                          title={t.hint}
                          onclick={() => void connectLocal(t)}
                          style={engineStagger
                            ? `animation-delay: ${Math.min(i, 8) * 35}ms`
                            : ""}
                          class={cn(
                            "group flex h-12 items-center gap-2 rounded-lg border px-2.5 text-left outline-none transition-[color,background-color,border-color,scale] duration-150 ease-out",
                            engineStagger && "cn-stagger-in",
                            t.conn
                              ? "border-border/60 bg-card/40 text-foreground hover:border-border hover:bg-accent/40 focus-visible:border-ring active:scale-[0.98] disabled:opacity-60"
                              : "cursor-not-allowed border-border/40 opacity-50",
                          )}
                        >
                          {#if busy}
                            <Icon
                              name="loader-2"
                              class="size-4 shrink-0 animate-spin"
                            />
                          {:else if t.conn}
                            <DbIcon
                              id={t.mark}
                              class={cn("size-4 shrink-0", engineTint(t.mark))}
                            />
                          {:else}
                            <!-- Unresolvable studio: a warning mark reads as
                                 "needs attention", not a greyed-out database. -->
                            <Icon
                              name="alert-triangle"
                              class="size-4 shrink-0 text-warning"
                            />
                          {/if}
                          <span class="min-w-0 flex-1">
                            <span class="block truncate text-ui-2xs font-medium"
                              >{t.title}</span
                            >
                            <!-- A resolved target shows its URL on one line; a
                                 failed one shows why, wrapped so it stays
                                 readable instead of cut off mid-sentence. -->
                            <span
                              class={cn(
                                "mt-0.5 block font-mono text-ui-3xs leading-tight tabular-nums text-muted-foreground",
                                t.conn ? "truncate" : "line-clamp-2",
                              )}
                              >{t.subtitle}</span
                            >
                          </span>
                          {#if t.trailingMark}
                            <span
                              class="flex size-5 shrink-0 items-center justify-center rounded border border-border/40 bg-muted/40"
                            >
                              <DbIcon
                                id={t.trailingMark}
                                class={cn("size-3", engineTint(t.trailingMark))}
                              />
                            </span>
                          {/if}
                        </button>
                      {/each}
                    </div>
                  </div>
                {/snippet}

                {#if dockerTargets.length > 0}
                  {@render localGroup("Docker", dockerTargets, false, "docker")}
                {/if}

                  <!-- Everything else: the form, on its own engine dropdown.
                       This used to be a second page of seventeen driver tiles. -->
                  <div>
                    <h3 class="text-ui-xs font-medium text-foreground/80">
                      Something else
                    </h3>
                    <!-- A tile in the provider grid's own columns and in the provider
                         cards' own style (border, fill, hover, press), so it reads as
                         one more card in the same set rather than a different kind of
                         control. It was a primary Button, whose label rendered
                         unreadable on this surface. -->
                    <div class="mt-2 grid grid-cols-2 gap-1.5 @2xl:grid-cols-4">
                      <button
                        type="button"
                        onclick={newConnectionForm}
                        title="Pick an engine and enter its details"
                        class="group flex h-9 min-w-0 items-center gap-2 rounded-lg border border-border/60 bg-card/40 px-2.5 text-left text-foreground outline-none transition-[color,background-color,border-color,scale] duration-150 ease-out hover:border-border hover:bg-accent/40 focus-visible:border-ring active:scale-[0.98]"
                      >
                        <Icon name="plus" class="size-4 shrink-0 text-muted-foreground transition-colors group-hover:text-foreground" aria-hidden="true" />
                        <span class="min-w-0 flex-1 truncate text-ui-2xs font-medium">New connection</span>
                        <Kbd combo="Mod+N" size="sm" class="shrink-0 text-muted-foreground" />
                      </button>
                    </div>
                  </div>

              </div>
            </ScrollArea>

            <!-- ── Step 2 · details for the chosen database ── -->
          {:else}
            <ScrollArea type="auto" class="min-h-0 flex-1 scroll-smooth">
              <!-- No column cap on this step. It had one, at 42rem and then at
                   64rem, because the fields used to invent their own widths and
                   a wide window strung five of them out with nothing lining up.
                   They share one 12-column grid now, so width only makes the
                   same layout roomier - and the panel already runs the header,
                   the footer and the URL bar edge to edge, so a capped form in
                   the middle of them was the odd one out. -->
              <div class="flex w-full items-start gap-10 px-8 py-5">
                <!-- Enter connects, from any field - filling a form and having to go
                 find the button is the one interaction nobody expects here. The
                 paste bar's own Enter handler runs first and marks the event
                 handled, so pasting a URI still just fills the fields. -->
                <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
                <div
                  class="@container min-w-0 w-full"
                  onkeydown={(e) => {
                    if (e.key !== "Enter" || e.defaultPrevented || isBusy)
                      return;
                    if (!(e.target instanceof HTMLInputElement)) return;
                    e.preventDefault();
                    void handleConnect();
                  }}
                  role="group"
                >
                  {#if entryMode === "manual"}
                    <!-- Manual form, core connection fields, then a full-width Advanced
                     section at the bottom. -->
                      <div class="flex min-w-0 flex-col gap-4">
                        <!-- The form reads as three parts, in the order the work actually happens:
                             say where the database is, adjust how you reach it, then name it. Name
                             used to hold the most prominent slot on the page despite being the one
                             optional field on it, and the connection string sat beside the six
                             fields it fills, so the same address was on screen twice. -->

                        <!-- ── 1 · Address ──────────────────────────────────────────────────── -->
                        <section>
                          <!-- Driver-specific fields. No caption: the labels say
                               what this is, and the engine is named in the
                               dropdown above. A URL fills these, from the bar
                               the footer opens. -->
                      {#key dbType}
                        <div class="flex flex-col gap-3.5">
                          <!-- ── PostgreSQL / CockroachDB / MySQL / MariaDB ── -->
                          {#if dbType === "postgres" || dbType === "cockroachdb" || dbType === "mysql" || dbType === "mariadb"}
                            <!-- Host, port, database, user and password are one
              address, so they share one row where the panel is wide enough and
              break 3+2 where it is not. -->
                            <!-- One row at 64rem: host 3 · port 2 · database 3 ·
                                 user 2 · password 2, the five parts of one
                                 address, breaking 3+3 only when the panel is
                                 narrow. Port held one twelfth of the track and
                                 clipped its own value at 5432. -->
                            <div class={cn(row6, "gap-y-4 @3xl:grid-cols-12")}>
                              <div class="col-span-3 min-w-0 sm:col-span-2 @3xl:col-span-3">
                                <label for="cn-host" class={lbl}>Host</label>
                                <Input
                                  id="cn-host"
                                  bind:value={host}
                                  class={cn(
                                    inp,
                                    flashedFields.has("host") && flashCls,
                                  )}
                                />
                              </div>
                              <div class="col-span-3 min-w-0 sm:col-span-1 @3xl:col-span-2">
                                <label for="cn-port" class={lbl}>Port</label>
                                <Input
                                  id="cn-port"
                                  bind:value={port}
                                  type="text"
                                  inputmode="numeric"
                                  class={cn(
                                    inpNum,
                                    flashedFields.has("port") && flashCls,
                                  )}
                                />
                              </div>
                              <div class="col-span-6 min-w-0 sm:col-span-3 @3xl:col-span-3">
                                <label for="cn-db" class={lbl}>Database</label>
                                <Input
                                  id="cn-db"
                                  bind:value={database}
                                  class={cn(
                                    inp,
                                    flashedFields.has("database") && flashCls,
                                  )}
                                />
                              </div>
                              <div class="col-span-6 min-w-0 sm:col-span-3 @3xl:col-span-2">
                                <label for="cn-user" class={lbl}>Username</label
                                >
                                <Input
                                  id="cn-user"
                                  bind:value={user}
                                  autocomplete="username"
                                  class={cn(
                                    inp,
                                    flashedFields.has("user") && flashCls,
                                  )}
                                />
                              </div>
                              <div class="col-span-6 min-w-0 sm:col-span-3 @3xl:col-span-2">
                                <label for="cn-pass" class={lbl}>Password</label
                                >
                                <PasswordInput
                                  id="cn-pass"
                                  bind:value={password}
                                  autocomplete="current-password"
                                  class={cn(
                                    inp,
                                    flashedFields.has("password") && flashCls,
                                  )}
                                />
                              </div>
                            </div>

                            <!-- ── SQLite ────────────────────────────────── -->
                          {:else if dbType === "sqlite"}
                            <!-- Local file vs remote SQLite (Turso / libSQL). Remote reuses the
                   dedicated libsql driver + backend by switching dbType. -->
                            <div
                              class="flex gap-0.5 rounded-md border border-border/25 bg-muted/30 p-0.5 text-ui-2xs"
                            >
                              <button
                                type="button"
                                class="flex-1 rounded bg-background px-2 py-1 font-medium text-foreground"
                                >Local file</button
                              >
                              <button
                                type="button"
                                onclick={() => (dbType = "libsql")}
                                class="flex-1 rounded px-2 py-1 text-muted-foreground transition-colors hover:text-foreground"
                                >Remote (Turso / libSQL)</button
                              >
                            </div>

                            <div>
                              <label for="cn-path" class={lbl}>File</label>
                              <div class="flex gap-1.5">
                                <Input
                                  id="cn-path"
                                  bind:value={filePath}
                                  placeholder="/path/to/database.db"
                                  class={cn(inp, "font-mono text-ui-2xs")}
                                />
                                <button
                                  type="button"
                                  onclick={pickSqliteFile}
                                  class="field-surface inline-flex h-8 shrink-0 items-center gap-1 px-2.5 text-ui-2xs text-muted-foreground transition-[color,background-color,border-color,transform] duration-150 ease-out hover:bg-muted/40 hover:text-foreground active:scale-[0.97]"
                                >
                                  <Icon name="folder-open" class="size-3" />
                                  Browse
                                </button>
                              </div>
                            </div>

                            <!-- ── In-Memory ─────────────────────────────── -->
                          {:else if dbType === "sqlite-memory"}
                            <div
                              class="flex flex-col gap-1.5 rounded-lg border border-border/15 bg-muted/[0.04] px-4 py-3.5"
                            >
                              <p
                                class="text-ui-xs font-medium text-foreground/70"
                              >
                                Ephemeral in-memory database
                              </p>
                              <p
                                class="text-ui-2xs leading-relaxed text-muted-foreground"
                              >
                                Data lives only for this session. Nothing is
                                written to disk, closing the connection discards
                                everything.
                              </p>
                            </div>

                            <!-- ── LibSQL / Turso ─────────────────────────── -->
                          {:else if dbType === "libsql"}
                            <div
                              class="flex gap-0.5 rounded-md border border-border/25 bg-muted/30 p-0.5 text-ui-2xs"
                            >
                              <button
                                type="button"
                                onclick={() => (dbType = "sqlite")}
                                class="flex-1 rounded px-2 py-1 text-muted-foreground transition-colors hover:text-foreground"
                                >Local file</button
                              >
                              <button
                                type="button"
                                class="flex-1 rounded bg-background px-2 py-1 font-medium text-foreground"
                                >Remote (Turso / libSQL)</button
                              >
                            </div>

                            <div>
                              <label for="cn-libsql-url" class={lbl}>URL</label>
                              <Input
                                id="cn-libsql-url"
                                bind:value={libsqlUrl}
                                placeholder="libsql://your-db.turso.io"
                                class={cn(inp, "font-mono text-ui-2xs")}
                              />
                              <p
                                class="mt-1 text-ui-3xs text-muted-foreground"
                              >
                                libsql:// · https:// · http://localhost:PORT
                              </p>
                            </div>

                            <div>
                              <label for="cn-libsql-token" class={lbl}
                                >Auth token <span
                                  class="normal-case font-normal opacity-50"
                                  >(optional)</span
                                ></label
                              >
                              <PasswordInput
                                id="cn-libsql-token"
                                bind:value={libsqlToken}
                                placeholder="eyJhbGciOiJFZERTQSJ9…"
                                class={cn(inp, "font-mono text-ui-2xs")}
                              />
                            </div>

                            <!-- ── ClickHouse ─────────────────────────────── -->
                          {:else if dbType === "clickhouse"}
                            <div class="grid grid-cols-[1fr_110px] gap-2">
                              <div>
                                <label for="cn-ch-host" class={lbl}>Host</label>
                                <Input
                                  id="cn-ch-host"
                                  bind:value={host}
                                  class={cn(
                                    inp,
                                    flashedFields.has("host") && flashCls,
                                  )}
                                />
                              </div>
                              <div>
                                <label for="cn-ch-port" class={lbl}>Port</label>
                                <Input
                                  id="cn-ch-port"
                                  bind:value={port}
                                  type="text"
                                  inputmode="numeric"
                                  class={cn(
                                    inpNum,
                                    flashedFields.has("port") && flashCls,
                                  )}
                                />
                              </div>
                            </div>

                            <div>
                              <label for="cn-ch-db" class={lbl}>Database</label>
                              <Input
                                id="cn-ch-db"
                                bind:value={database}
                                class={cn(
                                  inp,
                                  flashedFields.has("database") && flashCls,
                                )}
                              />
                            </div>

                            <div class="grid grid-cols-2 gap-2">
                              <div>
                                <label for="cn-ch-user" class={lbl}
                                  >Username</label
                                >
                                <Input
                                  id="cn-ch-user"
                                  bind:value={user}
                                  autocomplete="username"
                                  class={cn(
                                    inp,
                                    flashedFields.has("user") && flashCls,
                                  )}
                                />
                              </div>
                              <div>
                                <label for="cn-ch-pass" class={lbl}
                                  >Password</label
                                >
                                <PasswordInput
                                  id="cn-ch-pass"
                                  bind:value={password}
                                  autocomplete="current-password"
                                  class={cn(
                                    inp,
                                    flashedFields.has("password") && flashCls,
                                  )}
                                />
                              </div>
                            </div>

                            <!-- ── DuckDB (file) ──────────────────────────── -->
                          {:else if dbType === "duckdb"}
                            <div>
                              <label for="cn-duck-path" class={lbl}>File</label>
                              <div class="flex gap-1.5">
                                <Input
                                  id="cn-duck-path"
                                  bind:value={filePath}
                                  placeholder="/path/to/database.duckdb"
                                  class={cn(inp, "font-mono text-ui-2xs")}
                                />
                                <button
                                  type="button"
                                  onclick={pickDuckdbFile}
                                  class="field-surface inline-flex h-8 shrink-0 items-center gap-1 px-2.5 text-ui-2xs text-muted-foreground transition-colors hover:bg-muted/40 hover:text-foreground"
                                >
                                  <Icon name="folder-open" class="size-3" />
                                  Browse
                                </button>
                              </div>
                              <p
                                class="mt-1 text-ui-3xs text-muted-foreground"
                              >
                                A new file is created if it doesn't exist.
                              </p>
                            </div>

                            <!-- ── DuckDB (in-memory) ─────────────────────── -->
                          {:else if dbType === "duckdb-memory"}
                            <div
                              class="flex flex-col gap-1.5 rounded-lg border border-border/15 bg-muted/[0.04] px-4 py-3.5"
                            >
                              <p
                                class="text-ui-xs font-medium text-foreground/70"
                              >
                                Ephemeral in-memory DuckDB
                              </p>
                              <p
                                class="text-ui-2xs leading-relaxed text-muted-foreground"
                              >
                                A columnar analytical database that lives only
                                for this session. Nothing is written to disk,
                                closing the connection discards everything.
                              </p>
                            </div>

                            <!-- ── MS SQL Server ──────────────────────────── -->
                          {:else if dbType === "mssql"}
                            <div class="grid grid-cols-[1fr_110px] gap-2">
                              <div>
                                <label for="cn-mssql-host" class={lbl}
                                  >Host</label
                                >
                                <Input
                                  id="cn-mssql-host"
                                  bind:value={host}
                                  class={cn(
                                    inp,
                                    flashedFields.has("host") && flashCls,
                                  )}
                                />
                              </div>
                              <div>
                                <label for="cn-mssql-port" class={lbl}
                                  >Port</label
                                >
                                <Input
                                  id="cn-mssql-port"
                                  bind:value={port}
                                  type="text"
                                  inputmode="numeric"
                                  class={cn(
                                    inpNum,
                                    flashedFields.has("port") && flashCls,
                                  )}
                                />
                              </div>
                            </div>

                            <div>
                              <label for="cn-mssql-db" class={lbl}
                                >Database</label
                              >
                              <Input
                                id="cn-mssql-db"
                                bind:value={database}
                                class={cn(
                                  inp,
                                  flashedFields.has("database") && flashCls,
                                )}
                              />
                            </div>

                            <div class="grid grid-cols-2 gap-2">
                              <div>
                                <label for="cn-mssql-user" class={lbl}
                                  >Username</label
                                >
                                <Input
                                  id="cn-mssql-user"
                                  bind:value={user}
                                  autocomplete="username"
                                  class={cn(
                                    inp,
                                    flashedFields.has("user") && flashCls,
                                  )}
                                />
                              </div>
                              <div>
                                <label for="cn-mssql-pass" class={lbl}
                                  >Password</label
                                >
                                <PasswordInput
                                  id="cn-mssql-pass"
                                  bind:value={password}
                                  autocomplete="current-password"
                                  class={cn(
                                    inp,
                                    flashedFields.has("password") && flashCls,
                                  )}
                                />
                              </div>
                            </div>

                            <!-- ── Redis ──────────────────────────────────── -->
                          {:else if dbType === "redis"}
                            <div class="grid grid-cols-[1fr_110px] gap-2">
                              <div>
                                <label for="cn-redis-host" class={lbl}
                                  >Host</label
                                >
                                <Input
                                  id="cn-redis-host"
                                  bind:value={host}
                                  class={cn(
                                    inp,
                                    flashedFields.has("host") && flashCls,
                                  )}
                                />
                              </div>
                              <div>
                                <label for="cn-redis-port" class={lbl}
                                  >Port</label
                                >
                                <Input
                                  id="cn-redis-port"
                                  bind:value={port}
                                  type="text"
                                  inputmode="numeric"
                                  class={cn(
                                    inpNum,
                                    flashedFields.has("port") && flashCls,
                                  )}
                                />
                              </div>
                            </div>

                            <div>
                              <label for="cn-redis-pass" class={lbl}
                                >Password <span
                                  class="normal-case font-normal opacity-50"
                                  >(optional)</span
                                ></label
                              >
                              <PasswordInput
                                id="cn-redis-pass"
                                bind:value={password}
                                autocomplete="current-password"
                                class={cn(
                                  inp,
                                  flashedFields.has("password") && flashCls,
                                )}
                              />
                            </div>

                            <div>
                              <label for="cn-redis-db" class={lbl}
                                >Database index</label
                              >
                              <Input
                                id="cn-redis-db"
                                bind:value={database}
                                type="text"
                                inputmode="numeric"
                                min="0"
                                max="15"
                                class={inpNum}
                              />
                              <p
                                class="mt-1 text-ui-3xs text-muted-foreground"
                              >
                                Logical database number (0-15).
                              </p>
                            </div>

                            <label
                              class="flex cursor-pointer select-none items-center gap-2"
                            >
                              <Checkbox
                                id="cn-redis-tls"
                                checked={secure}
                                onCheckedChange={(v) => (secure = v === true)}
                              />
                              <span class="text-ui-xs text-muted-foreground"
                                >Use TLS</span
                              >
                            </label>
                          {/if}
                        </div>
                      {/key}
                        </section>

                        <!-- ── 2 · Transport and name ──────────────────────────────────────
                             Three checkboxes and one optional text field, which is one
                             band of form between them - they had a rule and a section
                             each, for four controls. Name still comes last in reading
                             order: it is the only optional field on the page, and it
                             used to hold the first and most prominent slot. -->
                        <section class="border-t border-border/40 pt-3.5">
                          <!-- Transport above, name below: at 42rem they are two
                               bands in reading order rather than two columns, and
                               Name stops floating off to the right of the SSH
                               fields it has nothing to do with. -->
                          <div class="flex flex-col gap-5">
                            <div class="min-w-0">
                              {@render advancedFields()}
                            </div>
                            <div class="min-w-0 max-w-[20rem]">
                              <label for="cn-name" class={lbl}>
                                Name
                                <span class="font-normal text-muted-foreground"
                                  >· optional</span
                                >
                              </label>
                              <Input
                                id="cn-name"
                                bind:value={name}
                                class={inp}
                                placeholder={autoName}
                              />
                            </div>
                          </div>
                        </section>
                      </div>
                    <!-- /manual column -->
                  {:else if isProvider}
                    <div class="mt-6 flex flex-col gap-4">
                      <!-- Keyed on the provider so switching fully remounts the flow:
                         re-checks sign-in status and clears the previous provider's
                         database list (otherwise a stale pick hits the wrong account). -->
                      {#key dbType}
                        <ProviderConnect
                          provider={dbType}
                          resolvePassword={(host, user) =>
                            saved.find(
                              (s) =>
                                s.host === host &&
                                s.user === user &&
                                s.password,
                            )?.password}
                          resolveSavedConnection={(ref, dbName) =>
                            // By the provider's own reference first. Entries
                            // saved before it was recorded match on an exact
                            // database name only: the old `name.endsWith(db)`
                            // test matched a database called "app" to a saved
                            // "PlanetScale · myapp" and connected to the wrong one.
                            saved.find(
                              (s) =>
                                s.provider === dbType &&
                                (s.password || s.authToken) &&
                                (s.providerRef
                                  ? s.providerRef === ref
                                  : !!dbName && s.database === dbName),
                            )}
                          onselect={(conn) => connectProviderConnection(conn)}
                        />
                      {/key}
                      {@render readOnlyRow()}
                    </div>
                  {:else if dbType === "d1"}
                    <div class="mt-6 flex flex-col gap-4">
                      {#key dbType}
                        <CloudflareLogin
                          onselect={connectD1Selection}
                          ondisconnect={() => {
                            accountId = "";
                            databaseId = "";
                            apiToken = "";
                          }}
                          initialAccountId={accountId}
                          initialDatabaseId={databaseId}
                          initialDatabaseName={editingId ? name : ""}
                        />
                      {/key}

                      <details class="group">
                        <summary
                          class="flex cursor-pointer list-none select-none items-center gap-1 text-ui-2xs text-muted-foreground transition-colors hover:text-muted-foreground"
                        >
                          <Icon
                            name="chevron-right"
                            class="size-3 transition-transform duration-150 group-open:rotate-90"
                          />
                          Enter account &amp; token manually
                        </summary>
                        <div class="mt-3 flex flex-col gap-2.5">
                          <div class="grid grid-cols-2 gap-2">
                            <div>
                              <label for="cn-d1-account" class={lbl}
                                >Account ID</label
                              >
                              <Input
                                id="cn-d1-account"
                                bind:value={accountId}
                                placeholder="abcdef…"
                                class={cn(inp, "font-mono text-ui-2xs")}
                              />
                            </div>
                            <div>
                              <label for="cn-d1-dbid" class={lbl}
                                >Database ID</label
                              >
                              <Input
                                id="cn-d1-dbid"
                                bind:value={databaseId}
                                placeholder="xxxxxxxx-…"
                                class={cn(inp, "font-mono text-ui-2xs")}
                              />
                            </div>
                          </div>
                          <div>
                            <label for="cn-d1-token" class={lbl}
                              >API token</label
                            >
                            <PasswordInput
                              id="cn-d1-token"
                              bind:value={apiToken}
                              class={inp}
                            />
                          </div>
                        </div>
                      </details>
                      {@render readOnlyRow()}
                    </div>
                  {/if}
                </div>
              </div>
            </ScrollArea>
          {/if}

          <!-- ── URL bar, above the footer and under the control that opens it ──
               It was inline at the top of the form, opened by a button in the
               header. On a form long enough to scroll that is a control at one
               end of the page and its effect at the other: you press "Use URL"
               and, as far as you can see, nothing happens. Here the bar rises
               out of the footer the button sits in. -->
          {#if step === "form" && entryMode === "manual" && hasFieldToggle && importOpen}
            <div id="cn-url-bar" class="shrink-0 border-t border-border/15 bg-card/30 px-8 py-3">
              <div class="flex items-stretch gap-2">
                <div class="min-w-0 flex-1">
                  <label for="cn-import-uri" class="sr-only">Connection URL</label>
                  <Input
                    id="cn-import-uri"
                    bind:value={importUri}
                    placeholder={uriPlaceholder}
                    spellcheck="false"
                    autocomplete="off"
                    aria-describedby="cn-import-hint"
                    aria-invalid={uriHint && (uriHint.includes("Could") || uriHint.includes("Expected")) ? "true" : undefined}
                    class={cn(inp, "font-mono text-ui-2xs")}
                    onkeydown={(e) => {
                      if (e.key === "Enter") {
                        e.preventDefault();
                        runImport();
                        return;
                      }
                      if (e.key === "Escape") {
                        e.preventDefault();
                        e.stopPropagation();
                        importOpen = false;
                        importUri = "";
                        uriHint = "";
                      }
                    }}
                  />
                </div>
                <button
                  type="button"
                  disabled={!importUri.trim()}
                  onclick={runImport}
                  class="inline-flex h-9 shrink-0 items-center gap-1.5 rounded-[10px] bg-primary px-3.5 text-ui-2xs font-medium text-primary-foreground outline-none transition-[opacity,transform] hover:opacity-90 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring active:scale-[0.96] disabled:pointer-events-none disabled:opacity-40"
                >
                  Use URL
                </button>
              </div>
              <p
                id="cn-import-hint"
                class={cn(
                  "mt-1.5 flex items-start gap-1.5 text-ui-3xs leading-relaxed",
                  !uriHint
                    ? "text-muted-foreground"
                    : uriHint.includes("Could") || uriHint.includes("Expected")
                      ? "text-destructive"
                      : "text-success",
                )}
              >
                {#if uriHint}
                  <Icon
                    name={uriHint.includes("Could") || uriHint.includes("Expected") ? "alert-circle" : "check-circle-2"}
                    class="mt-px size-3 shrink-0"
                    aria-hidden="true"
                  />
                {/if}
                <span class="min-w-0">
                  {uriHint || "Fills the fields above, and switches the engine if the URL is for another one."}
                </span>
              </p>
            </div>
          {/if}

          <!-- ── Footer: inline error alert, status chip, then actions ── -->
          <div class="shrink-0 border-t border-border/15 px-8 py-4">
            <div class="mx-auto max-w-none">
              <!-- The alert itself is a toast (see failWith). What stays here is the
                 one-line record of it, so the driver's own words are still
                 readable after the toast has gone - without a panel that shoves
                 the form upward every time a connection fails. -->

              <div class="flex items-center gap-3">
                <!-- Status chip + subtle target preview. Step 1 has no target yet,
                   so it shows nothing rather than "Ready" for a database nobody
                   has chosen. -->
                <div
                  class="flex min-w-0 flex-1 items-center gap-2 text-ui-2xs"
                  role="status"
                  aria-live="polite"
                >
                  {#if step === "pick"}
                    <span class="text-ui-2xs text-muted-foreground">
                      {saved.length === 0
                        ? "Everything stays on this machine"
                        : railOpen
                          ? "Or pick a saved connection on the left"
                          : "Saved connections are hidden - reopen them from the top left"}
                    </span>
                  {:else if connecting}
                    <span
                      class="flex shrink-0 items-center gap-1.5 font-medium text-muted-foreground"
                      ><Icon
                        name="loader-2"
                        class="size-3 animate-spin"
                      />Connecting…</span
                    >
                  {:else if testing}
                    <span
                      class="flex shrink-0 items-center gap-1.5 font-medium text-muted-foreground"
                      ><Icon
                        name="loader-2"
                        class="size-3 animate-spin"
                      />Testing…</span
                    >
                  {:else if error}
                    <span
                      class="flex shrink-0 items-center gap-1.5 font-medium text-destructive"
                      ><span class="size-1.5 rounded-full bg-destructive"
                      ></span>Failed</span
                    >
                    <span
                      class="min-w-0 flex-1 truncate font-mono text-ui-2xs text-muted-foreground select-text"
                      title={error}
                      data-studio-selectable="text">{error}</span
                    >
                  {:else if testOk}
                    <span
                      class="flex shrink-0 items-center gap-1.5 font-medium text-success"
                      ><span class="size-1.5 rounded-full bg-success"
                      ></span>Connection OK</span
                    >
                  {:else if isDirty}
                    <span
                      class="flex shrink-0 items-center gap-1.5 font-medium text-warning"
                      ><span class="size-1.5 rounded-full bg-warning"
                      ></span>Unsaved</span
                    >
                  {:else}
                    <span
                      class="flex shrink-0 items-center gap-1.5 text-muted-foreground"
                      ><span
                        class="size-1.5 rounded-full bg-muted-foreground/30"
                      ></span>Ready</span
                    >
                  {/if}
                  {#if step !== "pick" && !error}
                    <span
                      class="min-w-0 truncate font-mono text-ui-2xs text-muted-foreground"
                      title={statusTarget}>{statusTarget}</span
                    >
                  {/if}
                </div>

                <!-- Actions, shared Button variants (Resume ghost · Stop soft-destructive
                   · Test outline · Connect solid primary), one system app-wide. -->
                <!-- Two groups, spaced apart rather than run together. Disconnect and
                   Resume act on the live session; Test, Save and Connect act on the
                   form in front of you. They read as one undifferentiated row of five
                   buttons at a single gap, which is how "Resume" ends up looking like
                   a peer of "Connect". 4px inside a group against 20px between them
                   is the 1:5 ratio that makes the split legible without a divider. -->
                <div class="ml-auto flex shrink-0 items-center gap-5">
                  {#if activeConnectionName || (lastId && saved.find((c) => c.id === lastId))}
                  <div class="flex min-w-0 items-center gap-1">
                  <!-- Ending the live session belongs next to resuming it: both act
                     on the current connection rather than on the form. -->
                  {#if activeConnectionName}
                    <Button
                      variant="ghost"
                      class="max-w-[220px] text-muted-foreground hover:text-destructive"
                      disabled={isBusy}
                      title="Disconnect {activeConnectionName}"
                      onclick={() => {
                        open = false;
                        ondisconnect();
                      }}
                    >
                      <Icon name="unplug" class="size-3.5" />Disconnect
                    </Button>
                  {/if}
                  {#if lastId && saved.find((c) => c.id === lastId)}
                    {@const lastConn = saved.find((c) => c.id === lastId)}
                    <Button
                      variant="ghost"
                      class="max-w-[200px] text-muted-foreground"
                      disabled={isBusy}
                      title="Resume {lastConn.name} ({IS_MAC ? '⌘⇧' : 'Ctrl+Shift+'}Enter)"
                      aria-keyshortcuts="Control+Shift+Enter Meta+Shift+Enter"
                      onclick={() => connectWith(lastConn)}
                    >
                      {#if connecting === lastConn.id}
                        <Icon
                          name="loader-2"
                          class="size-3.5 animate-spin"
                        />Resuming…
                      {:else}
                        Resume <span class="min-w-0 truncate text-foreground/80"
                          >{lastConn.name}</span
                        >
                      {/if}
                    </Button>
                  {/if}
                  </div>
                  {/if}
                  <div class="flex shrink-0 items-center gap-2">
                  {#if isBusy}
                    <Button variant="destructive" onclick={stopOp}>
                      <Icon name="x" class="size-3.5" />Stop
                    </Button>
                  {/if}
                  <!-- Test / Save / Connect belong to a chosen database. On step 1
                     the only action that makes sense is resuming the last one. -->
                  {#if step === "form"}
                    <Button
                      variant="outline"
                      disabled={isBusy}
                      onclick={handleTest}
                    >
                      {#if testing}<Icon
                          name="loader-2"
                          class="size-3.5 animate-spin"
                        />Testing…{:else}Test{/if}
                    </Button>
                    <!-- Save only: persists the form and leaves the active session
                     alone. Paired with the solid Connect button beside it. -->
                    <Button
                      variant="outline"
                      disabled={isBusy}
                      onclick={handleSave}
                    >
                      {#if justSaved}<Icon
                          name="check"
                          class="size-3.5 text-success"
                        />Saved{:else}Save{/if}
                    </Button>
                    {#if editingId}
                      <!-- Connect without writing anything. The primary beside it
                           saves the form first, which is what you want after an
                           edit - this is for the other case: open the connection
                           that is already on file and leave it alone. -->
                      <Button
                        variant="outline"
                        disabled={isBusy}
                        title="Connect without saving changes"
                        onclick={() => void handleConnect({ save: false })}
                      >
                        Connect
                      </Button>
                    {/if}
                    <Button
                      class={cn(
                        "px-5",
                        connecting === (editingId ?? "__new__") &&
                          "disabled:opacity-90",
                      )}
                      disabled={isBusy}
                      title="Connect (↵)"
                      onclick={() => void handleConnect()}
                    >
                      {#if connecting === (editingId ?? "__new__")}
                        <Icon
                          name="loader-2"
                          class="size-3.5 animate-spin"
                        />Connecting…
                      {:else}
                        {editingId ? "Save & connect" : "Connect"}
                      {/if}
                    </Button>
                  {/if}
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- Close button, routes through the unsaved-changes guard -->
      <button
        type="button"
        onclick={requestClose}
        class="field-surface absolute right-8 top-5 inline-flex size-8 items-center justify-center text-muted-foreground transition-[color,background-color,scale] duration-150 ease-out hover:bg-accent hover:text-foreground active:scale-[0.96]"
      >
        <Icon name="x" class="size-3.5" />
        <span class="sr-only">Close</span>
      </button>

      <!-- Duplicate-target confirmation. Same shape as the discard prompt: this
           is the other question the dialog can ask, and two different-looking
           confirmations in one dialog is one design too many. -->
      {#if duplicatePrompt}
        <div
          class="absolute inset-0 z-[60] flex items-center justify-center bg-black/65 p-6"
          role="alertdialog"
          aria-modal="true"
          aria-labelledby="cn-duplicate-title"
          tabindex="-1"
          use:focusTrap
        >
          <div class="w-full max-w-[26rem] rounded-[10px] border border-border/40 bg-background p-5 elevate-3-rim">
            <div class="flex items-start gap-3">
              <div class="mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-lg bg-warning/10 text-warning">
                <Icon name="alert-circle" class="size-4" />
              </div>
              <div class="min-w-0 flex-1">
                <!-- The name is data, not a heading. It used to be spliced into
                     the title, and a saved connection named after its RDS
                     endpoint turned one line into four before the question was
                     even reached. Title states the fact; the name sits under it
                     on its own line, clipped to two. -->
                <h3 id="cn-duplicate-title" class="text-ui-sm font-semibold text-foreground">
                  This database is already saved
                </h3>
                <p class="mt-1.5 line-clamp-2 break-all font-mono text-ui-2xs leading-snug text-foreground/70">
                  {duplicatePrompt.existing.name || 'Unnamed'}
                </p>
                <p class="mt-2 text-ui-xs leading-relaxed text-muted-foreground">
                  Same server, same database, same user. Save this one as well?
                </p>
              </div>
            </div>
            <div class="mt-5 flex justify-end gap-2">
              <button
                type="button"
                data-autofocus
                onclick={() => {
                  const existing = duplicatePrompt?.existing;
                  duplicatePrompt = null;
                  if (existing) resetForm(existing);
                }}
                class="field-surface inline-flex h-8 shrink-0 items-center px-3 text-ui-xs text-muted-foreground transition-[color,background-color,border-color,transform] duration-150 ease-out hover:bg-muted/40 hover:text-foreground active:scale-[0.97]"
              >
                Open it
              </button>
              <button
                type="button"
                onclick={() => {
                  const proceed = duplicatePrompt?.proceed;
                  duplicatePrompt = null;
                  duplicateAck = true;
                  proceed?.();
                }}
                class="inline-flex h-8 shrink-0 items-center rounded-lg bg-primary px-3.5 text-ui-xs font-semibold text-primary-foreground transition-[opacity,transform] duration-150 ease-out hover:opacity-90 active:scale-[0.97]"
              >
                Save anyway
              </button>
            </div>
          </div>
        </div>
      {/if}

      <!-- Discard-changes confirmation (styled, blocks close until answered) -->
      {#if confirmDiscardOpen}
        <div
          class="absolute inset-0 z-[60] flex items-center justify-center bg-black/65 p-6"
        >
          <div
            class="w-full max-w-sm rounded-[10px] border border-border/40 bg-background p-5 elevate-3-rim"
          >
            <div class="flex items-start gap-3">
              <div
                class="mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-lg bg-warning/10 text-warning"
              >
                <Icon name="alert-circle" class="size-4" />
              </div>
              <div class="min-w-0">
                <h3 class="text-ui-sm font-semibold text-foreground">
                  Discard unsaved changes?
                </h3>
                <p
                  class="mt-1 text-ui-xs leading-relaxed text-muted-foreground"
                >
                  You have unsaved edits in this connection. Closing now will
                  lose them.
                </p>
              </div>
            </div>
            <div class="mt-4 flex justify-end gap-2">
              <button
                type="button"
                onclick={() => (confirmDiscardOpen = false)}
                class="field-surface inline-flex h-8 items-center px-3.5 text-ui-xs text-muted-foreground transition-[color,background-color,border-color,transform] duration-150 ease-out hover:bg-muted/40 hover:text-foreground active:scale-[0.97]"
              >
                Keep editing
              </button>
              <button
                type="button"
                onclick={discardAndClose}
                class="inline-flex h-8 items-center rounded-lg bg-destructive px-3.5 text-ui-xs font-semibold text-white transition-[opacity,transform] duration-150 ease-out hover:opacity-90 active:scale-[0.97]"
              >
                Discard
              </button>
            </div>
          </div>
        </div>
      {/if}
    </DialogPrimitive.Content>
  </DialogPrimitive.Portal>
</DialogPrimitive.Root>

<ConfirmDialog
  bind:open={confirmDeleteOpen}
  icon="trash-2"
  title="Delete connection?"
  description={`"${pendingDelete?.name ?? ''}" will be removed from your saved connections.`}
  note="Its query history, saved queries, charts and AI chats are deleted with it. This can't be undone."
  confirmLabel="Delete"
  confirmIcon="trash-2"
  variant="destructive"
  onconfirm={confirmDelete}
  oncancel={() => (pendingDelete = null)}
/>

<style>
  /* The modal is portalled to <body>, outside #app, so it never inherits the
     app-wide `user-select: none`. Re-establish it here so chrome (labels, titles,
     rows, buttons, status) can't be drag-selected, while real field VALUES stay
     selectable. Scoped to this modal only via the data attribute. */
  :global([data-connection-modal]),
  :global([data-connection-modal] *) {
    user-select: none;
    -webkit-user-select: none;
  }
  :global([data-connection-modal] input),
  :global([data-connection-modal] textarea) {
    user-select: text;
    -webkit-user-select: text;
  }
  /* Pointer cursor on every interactive control in the modal (toggles, Type
     picker, rows, disclosure, footer buttons); disabled ones fall back. */
  :global([data-connection-modal] button:not(:disabled)),
  :global([data-connection-modal] [role="button"]),
  :global([data-connection-modal] summary) {
    cursor: pointer;
  }

  /* Decorative entrance for saved / provider rows - fades + rises in.
     Never blocks clicks (runs on the interactive element itself). */
  @keyframes -global-cn-rise-in {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
  :global(.cn-stagger-in) {
    opacity: 0;
    animation: cn-rise-in 250ms cubic-bezier(0.23, 1, 0.32, 1) both;
  }
  @media (prefers-reduced-motion: reduce) {
    :global(.cn-stagger-in) {
      animation: none;
      opacity: 1;
    }
  }
</style>
