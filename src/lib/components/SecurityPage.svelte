<script>
  import { executeSql } from "$lib/api.js";
  import ShieldCheck from "@lucide/svelte/icons/shield-check";
  import Users from "@lucide/svelte/icons/users";
  import Lock from "@lucide/svelte/icons/lock";
  import LockOpen from "@lucide/svelte/icons/lock-open";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import Plus from "@lucide/svelte/icons/plus";
  import Check from "@lucide/svelte/icons/check";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import KeyRound from "@lucide/svelte/icons/key-round";
  import { toast } from "$lib/components/ui/sonner/toast.svelte.js";
  import { cn } from "$lib/utils.js";
  import SecuritySqlModal from "./SecuritySqlModal.svelte";

  /** @type {{ active?: boolean, connectionType?: string | null }} */
  let { active = false, connectionType = null } = $props();

  // Roles + RLS use pg_catalog - only available on Postgres-compatible engines.
  const supported = $derived(
    connectionType === 'postgres' || connectionType === 'cockroachdb'
  );

  /** @type {'roles' | 'permissions' | 'policies' | 'rls'} */
  let activeTab = $state("roles");

  /** @type {Record<string, unknown>[] | null} */
  let roles = $state(null);
  let rolesLoading = $state(false);
  let rolesError = $state("");

  /** @type {Record<string, unknown>[] | null} */
  let policies = $state(null);
  let policiesLoading = $state(false);
  let policiesError = $state("");

  /** @type {Record<string, unknown>[] | null} */
  let rlsTables = $state(null);
  let rlsLoading = $state(false);
  let rlsError = $state("");

  /** @type {{ title: string } | null} */
  let sqlModal = $state(null);
  let sqlDraft = $state("");
  let sqlRunning = $state(false);

  /** @type {number | null} */
  let expandedPolicy = $state(null);

  // ── Permissions (master-detail) ──────────────────────────────────────────
  /** Selected role name for the Permissions view. */
  let selectedRole = $state("");
  /** @type {Record<string, unknown>[] | null} pg_auth_members flattened → { member, granted } */
  let memberships = $state(null);
  let membershipsLoading = $state(false);
  /** @type {Record<string, unknown>[] | null} per-database privileges for selectedRole */
  let dbGrants = $state(null);
  let dbGrantsLoading = $state(false);
  let dbGrantsError = $state("");

  /** @param {{ columns?: {name:string}[], rows?: unknown[][] } | null} result */
  function toRecords(result) {
    if (!result?.columns || !result?.rows) return [];
    const cols = result.columns.map((c) => c.name);
    return result.rows.map(
      (row) =>
        /** @type {Record<string, unknown>} */ (
          Object.fromEntries(cols.map((c, i) => [c, row[i]]))
        ),
    );
  }

  async function loadRoles() {
    rolesLoading = true;
    rolesError = "";
    try {
      roles = toRecords(
        await executeSql(`
        SELECT rolname, rolsuper, rolcreaterole, rolcreatedb, rolcanlogin,
               rolreplication, rolbypassrls, rolinherit, rolconnlimit, rolvaliduntil::text AS rolvaliduntil
        FROM pg_catalog.pg_roles ORDER BY rolcanlogin DESC, rolname
      `),
      );
    } catch (e) {
      rolesError = String(e);
    } finally {
      rolesLoading = false;
    }
  }

  /** SQL single-quoted string literal (escapes embedded quotes). */
  const sqlLit = (/** @type {unknown} */ v) => `'${String(v).replace(/'/g, "''")}'`;

  async function loadMemberships() {
    membershipsLoading = true;
    try {
      memberships = toRecords(
        await executeSql(`
        SELECT r.rolname AS member, g.rolname AS granted
        FROM pg_catalog.pg_auth_members m
        JOIN pg_catalog.pg_roles r ON r.oid = m.member
        JOIN pg_catalog.pg_roles g ON g.oid = m.roleid
      `),
      );
    } catch {
      memberships = [];
    } finally {
      membershipsLoading = false;
    }
  }

  /** @param {string} roleName */
  async function loadDbGrants(roleName) {
    if (!roleName) return;
    dbGrantsLoading = true;
    dbGrantsError = "";
    try {
      dbGrants = toRecords(
        await executeSql(`
        SELECT datname,
               has_database_privilege(${sqlLit(roleName)}, datname, 'CONNECT') AS can_connect,
               has_database_privilege(${sqlLit(roleName)}, datname, 'CREATE') AS can_create,
               has_database_privilege(${sqlLit(roleName)}, datname, 'TEMP') AS can_temp
        FROM pg_catalog.pg_database
        WHERE datistemplate = false
        ORDER BY datname
      `),
      );
    } catch (e) {
      dbGrants = [];
      dbGrantsError = String(e);
    } finally {
      dbGrantsLoading = false;
    }
  }

  /** @param {string} roleName */
  function selectRole(roleName) {
    if (!roleName || roleName === selectedRole) return;
    selectedRole = roleName;
    dbGrants = null;
    void loadDbGrants(roleName);
  }

  async function loadPolicies() {
    policiesLoading = true;
    policiesError = "";
    try {
      policies = toRecords(
        await executeSql(`
        SELECT schemaname, tablename, policyname, permissive,
               array_to_string(roles, ', ') AS roles, cmd, qual, with_check
        FROM pg_catalog.pg_policies ORDER BY schemaname, tablename, policyname
      `),
      );
    } catch (e) {
      policiesError = String(e);
    } finally {
      policiesLoading = false;
    }
  }

  async function loadRls() {
    rlsLoading = true;
    rlsError = "";
    try {
      rlsTables = toRecords(
        await executeSql(`
        SELECT n.nspname AS schema, c.relname AS table_name,
               c.relrowsecurity AS rls_enabled, c.relforcerowsecurity AS rls_forced
        FROM pg_catalog.pg_class c
        JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
        WHERE c.relkind = 'r' AND n.nspname NOT IN ('pg_catalog','information_schema','pg_toast')
        ORDER BY n.nspname, c.relname
      `),
      );
    } catch (e) {
      rlsError = String(e);
    } finally {
      rlsLoading = false;
    }
  }

  export function refresh() {
    if (activeTab === "roles") {
      roles = null;
      void loadRoles();
    } else if (activeTab === "permissions") {
      roles = null;
      memberships = null;
      void loadRoles();
      void loadMemberships();
      if (selectedRole) {
        dbGrants = null;
        void loadDbGrants(selectedRole);
      }
    } else if (activeTab === "policies") {
      policies = null;
      void loadPolicies();
    } else {
      rlsTables = null;
      void loadRls();
    }
  }

  async function runSqlModal() {
    if (!sqlDraft.trim()) return;
    sqlRunning = true;
    try {
      await executeSql(sqlDraft);
      toast.success("Executed successfully");
      sqlModal = null;
      refresh();
    } catch (e) {
      toast.error("Could not run statement", { description: String(e) });
    } finally {
      sqlRunning = false;
    }
  }

  function openRoleModal(
    /** @type {Record<string,unknown> | null} */ role = null,
  ) {
    if (role) {
      sqlDraft = `ALTER ROLE "${role.rolname}" WITH LOGIN;\n-- OPTIONS: SUPERUSER | CREATEDB | CREATEROLE | REPLICATION | BYPASSRLS\n-- DROP ROLE "${role.rolname}";`;
      sqlModal = { title: `Edit role, ${role.rolname}` };
    } else {
      sqlDraft = `CREATE ROLE new_role WITH LOGIN PASSWORD 'changeme';`;
      sqlModal = { title: "Create role" };
    }
  }

  function openPolicyModal() {
    sqlDraft = `CREATE POLICY policy_name\n  ON public.table_name\n  AS PERMISSIVE        -- or RESTRICTIVE\n  FOR ALL              -- SELECT | INSERT | UPDATE | DELETE\n  TO PUBLIC\n  USING (true)\n  WITH CHECK (true);`;
    sqlModal = { title: "Create policy" };
  }

  /** @param {Record<string,unknown>} row */
  function openRlsModal(row) {
    const on = bool(row.rls_enabled);
    sqlDraft = on
      ? `ALTER TABLE "${row.schema}"."${row.table_name}" DISABLE ROW LEVEL SECURITY;`
      : `ALTER TABLE "${row.schema}"."${row.table_name}" ENABLE ROW LEVEL SECURITY;`;
    sqlModal = {
      title: `${on ? "Disable" : "Enable"} RLS, ${row.schema}.${row.table_name}`,
    };
  }

  $effect(() => {
    if (!supported) return;
    if (activeTab === "roles" && roles === null && !rolesLoading) loadRoles();
    else if (activeTab === "policies" && policies === null && !policiesLoading)
      loadPolicies();
    else if (activeTab === "rls" && rlsTables === null && !rlsLoading)
      loadRls();
    else if (activeTab === "permissions") {
      if (roles === null && !rolesLoading) loadRoles();
      if (memberships === null && !membershipsLoading) loadMemberships();
    }
  });

  // Auto-select the first role once loaded on the Permissions tab.
  $effect(() => {
    if (activeTab !== "permissions" || !roles || !roles.length) return;
    if (!selectedRole || !roles.some((r) => r.rolname === selectedRole)) {
      selectRole(String(roles[0].rolname));
    }
  });

  const bool = (v) => v === true || v === "t" || v === "true" || v === 1;

  /** @param {Record<string,unknown>} r */
  const roleKind = (r) =>
    bool(r.rolsuper) ? "super" : bool(r.rolcanlogin) ? "login" : "group";

  /** Roles grouped for the Permissions tree (empty groups dropped). */
  const roleGroups = $derived.by(() => {
    const list = roles ?? [];
    return [
      { id: "super", label: "Superusers", items: list.filter((r) => roleKind(r) === "super") },
      { id: "login", label: "Login roles", items: list.filter((r) => roleKind(r) === "login") },
      { id: "group", label: "Group roles", items: list.filter((r) => roleKind(r) === "group") },
    ].filter((g) => g.items.length);
  });

  const selectedRoleObj = $derived(
    (roles ?? []).find((r) => r.rolname === selectedRole) ?? null,
  );
  /** Roles that selectedRole inherits from (is a member of). */
  const memberOf = $derived(
    (memberships ?? [])
      .filter((m) => m.member === selectedRole)
      .map((m) => String(m.granted)),
  );
  /** Roles that are members of selectedRole. */
  const membersOfSelected = $derived(
    (memberships ?? [])
      .filter((m) => m.granted === selectedRole)
      .map((m) => String(m.member)),
  );

  /** Attribute chips shown in the Role Attributes panel. */
  const ROLE_ATTRS = [
    { key: "rolcanlogin", label: "Can login" },
    { key: "rolsuper", label: "Superuser" },
    { key: "rolcreatedb", label: "Create DB" },
    { key: "rolcreaterole", label: "Create role" },
    { key: "rolreplication", label: "Replication" },
    { key: "rolbypassrls", label: "Bypass RLS" },
    { key: "rolinherit", label: "Inherit" },
  ];

  const isLoading = $derived(
    supported && (
      (activeTab === "roles" && (rolesLoading || roles === null)) ||
      (activeTab === "permissions" && (rolesLoading || roles === null || dbGrantsLoading)) ||
      (activeTab === "policies" && (policiesLoading || policies === null)) ||
      (activeTab === "rls" && (rlsLoading || rlsTables === null))
    )
  );

  const TABS = [
    { id: "roles", label: "Roles & Users" },
    { id: "permissions", label: "Permissions" },
    { id: "policies", label: "Policies" },
    { id: "rls", label: "Row Level Security" },
  ];
</script>

<svelte:window
  onkeydown={(e) => {
    if (!active || sqlModal) return;
    if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && e.key === "r") {
      e.preventDefault();
      refresh();
    }
  }}
/>

<div class="flex min-h-0 flex-1 flex-col overflow-hidden">
  {#if !supported}
    {@const engineLabel = {
      mysql: 'MySQL', mariadb: 'MariaDB', sqlite: 'SQLite', d1: 'Cloudflare D1',
      libsql: 'LibSQL', clickhouse: 'ClickHouse', duckdb: 'DuckDB', mssql: 'SQL Server',
    }[connectionType ?? ''] ?? (connectionType ?? 'This engine')}
    <div class="flex min-h-0 flex-1 flex-col items-center justify-center gap-4 p-8 text-center">
      <ShieldCheck class="size-10 shrink-0 text-muted-foreground" />
      <div class="space-y-1.5">
        <p class="font-mono text-ui font-medium text-foreground">{engineLabel} does not support roles or RLS</p>
        <p class="font-mono text-ui-xs text-muted-foreground">
          Row Level Security and role management require PostgreSQL or CockroachDB.<br />
          Use the SQL console to manage {engineLabel} users and permissions directly.
        </p>
      </div>
    </div>
  {:else}
  <!-- Tab row -->
  <div class="flex shrink-0 items-center border-b border-border bg-panel">
    {#each TABS as tab (tab.id)}
      <button
        type="button"
        class={cn(
          "relative flex h-8 items-center px-3 font-mono text-ui-xs transition-colors",
          activeTab === tab.id
            ? "text-foreground"
            : "text-muted-foreground hover:text-foreground",
        )}
        onclick={() => { activeTab = /** @type {any} */ (tab.id) }}
      >
        {#if activeTab === tab.id}
          <span class="absolute inset-x-0 bottom-0 h-px bg-primary" aria-hidden="true"></span>
        {/if}
        {tab.label}
      </button>
    {/each}
    <div class="flex-1"></div>
    <button
      type="button"
      class="mr-1 inline-flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
      title="Refresh (⌘R)"
      onclick={refresh}
    >
      <RefreshCw class={cn("size-3.5", isLoading && "animate-spin")} />
    </button>
  </div>

  <!-- Content -->
  <div class="app-scroll min-h-0 flex-1 overflow-auto [will-change:transform]">
    <!-- Roles -->
    {#if activeTab === "roles"}
      <div
        class="flex items-center justify-between border-b border-border/40 px-3 py-2"
      >
        <span class="font-mono text-ui-xs text-muted-foreground"
          >{roles?.length ?? 0} roles</span
        >
        <button
          type="button"
          class= "field-surface inline-flex h-7 items-center gap-1.5 bg-background px-2.5 font-mono text-ui-xs text-muted-foreground transition-colors hover:bg-accent/35 hover:text-foreground"
          onclick={() => openRoleModal()}
        >
          <Plus class="size-3.5 shrink-0" />New role
        </button>
      </div>

      {#if rolesLoading || roles === null}
        <div
          class="loading-reveal flex items-center justify-center gap-2 py-16 text-muted-foreground"
        >
          <RefreshCw class="size-4 shrink-0 animate-spin" /><span
            class="font-mono text-ui-sm">Loading…</span
          >
        </div>
      {:else if rolesError}
        <div class="px-4 py-8 text-center">
          <p class="font-mono text-ui-xs text-destructive">{rolesError}</p>
          <button
            onclick={() => {
              roles = null;
              loadRoles();
            }}
            class="mt-2 font-mono text-ui-xs text-muted-foreground underline"
            >Retry</button
          >
        </div>
      {:else if roles.length === 0}
        <div
          class="flex h-full flex-col items-center justify-center gap-3 py-16 text-center"
        >
          <Users class="size-10 shrink-0 text-muted-foreground" />
          <p class="font-mono text-ui text-muted-foreground">No roles found</p>
        </div>
      {:else}
        <table class="w-full text-ui-xs">
          <thead class="studio-chrome sticky top-0 z-10 bg-panel text-left">
            <tr class="border-b border-border/50">
              <th class="px-3 py-2 font-mono font-normal text-muted-foreground"
                >Name</th
              >
              <th
                class="px-3 py-2 text-center font-mono font-normal text-muted-foreground"
                title="Can login">Login</th
              >
              <th
                class="px-3 py-2 text-center font-mono font-normal text-muted-foreground"
                title="Superuser">Super</th
              >
              <th
                class="px-3 py-2 text-center font-mono font-normal text-muted-foreground"
                title="Create DB">CreateDB</th
              >
              <th
                class="px-3 py-2 text-center font-mono font-normal text-muted-foreground"
                title="Create Role">CreateRole</th
              >
              <th
                class="px-3 py-2 text-center font-mono font-normal text-muted-foreground"
                title="Bypass RLS">BypassRLS</th
              >
              <th class="px-3 py-2 font-mono font-normal text-muted-foreground"
                >Conn limit</th
              >
              <th class="px-3 py-2 font-mono font-normal text-muted-foreground"
                >Expires</th
              >
              <th class="w-9 px-2"></th>
            </tr>
          </thead>
          <tbody>
            {#each roles ?? [] as role (role.rolname)}
              {@const isSuper = bool(role.rolsuper)}
              {@const isLogin = bool(role.rolcanlogin)}
              <tr
                class="group/row border-b border-border/30 outline-none hover:bg-accent/25"
              >
                <td class="px-3 py-2">
                  <div class="flex items-center gap-2">
                    <span
                      class={cn(
                        "inline-flex size-6 shrink-0 items-center justify-center rounded-full font-mono text-ui-3xs font-semibold uppercase",
                        isSuper
                          ? "bg-destructive/10 text-destructive"
                          : isLogin
                            ? "bg-primary/10 text-primary"
                            : "bg-muted text-muted-foreground",
                      )}>{String(role.rolname).slice(0, 2)}</span
                    >
                    <span
                      class="font-mono text-ui-xs font-medium text-foreground"
                      >{role.rolname}</span
                    >
                    {#if isSuper}
                      <span
                        class="rounded bg-destructive/10 px-1.5 py-px font-mono text-ui-3xs font-medium text-destructive"
                        >superuser</span
                      >
                    {/if}
                  </div>
                </td>
                {#each [{ col: "rolcanlogin", c: "text-primary" }, { col: "rolsuper", c: "text-warning" }, { col: "rolcreatedb", c: "text-primary" }, { col: "rolcreaterole", c: "text-primary" }, { col: "rolbypassrls", c: "text-primary" }] as f (f.col)}
                  <td class="px-3 py-2 text-center">
                    {#if bool(role[f.col])}
                      <Check class={cn("mx-auto size-3.5", f.c)} />
                    {:else}
                      <span class="text-muted-foreground">—</span>
                    {/if}
                  </td>
                {/each}
                <td
                  class="px-3 py-2 font-mono text-ui-xs tabular-nums text-muted-foreground"
                >
                  {role.rolconnlimit === -1 || role.rolconnlimit == null
                    ? "∞"
                    : role.rolconnlimit}
                </td>
                <td
                  class="px-3 py-2 font-mono text-ui-xs text-muted-foreground"
                  >{role.rolvaliduntil ?? "—"}</td
                >
                <td class="px-2 py-2">
                  <button
                    type="button"
                    class="opacity-0 inline-flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground group-hover/row:opacity-100"
                    onclick={() => openRoleModal(role)}
                    title="Edit"><KeyRound class="size-3.5 shrink-0" /></button
                  >
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}

      <!-- Permissions (master-detail) -->
    {:else if activeTab === "permissions"}
      {#if rolesLoading || roles === null}
        <div class="loading-reveal flex items-center justify-center gap-2 py-16 text-muted-foreground">
          <RefreshCw class="size-4 shrink-0 animate-spin" /><span class="font-mono text-ui-sm">Loading…</span>
        </div>
      {:else if rolesError}
        <div class="px-4 py-8 text-center">
          <p class="font-mono text-ui-xs text-destructive">{rolesError}</p>
          <button onclick={() => { roles = null; loadRoles(); }} class="mt-2 font-mono text-ui-xs text-muted-foreground underline">Retry</button>
        </div>
      {:else}
        <div class="flex h-full min-h-0">
          <!-- Left: role tree -->
          <aside class="w-56 shrink-0 overflow-y-auto border-r border-border/40">
            {#each roleGroups as group (group.id)}
              <div class="px-3 pt-3 pb-1 font-mono text-ui-3xs uppercase tracking-[0.06em] text-muted-foreground">
                {group.label} · {group.items.length}
              </div>
              {#each group.items as r (r.rolname)}
                {@const isSuper = bool(r.rolsuper)}
                {@const isLogin = bool(r.rolcanlogin)}
                {@const isSel = selectedRole === r.rolname}
                <button
                  type="button"
                  class={cn(
                    "flex w-full items-center gap-2 px-3 py-1.5 text-left outline-none transition-colors",
                    isSel ? "bg-accent/40" : "hover:bg-accent/20",
                  )}
                  onclick={() => selectRole(String(r.rolname))}
                >
                  <span
                    class={cn(
                      "inline-flex size-5 shrink-0 items-center justify-center rounded-full font-mono text-ui-3xs font-semibold uppercase",
                      isSuper ? "bg-destructive/10 text-destructive" : isLogin ? "bg-primary/10 text-primary" : "bg-muted text-muted-foreground",
                    )}>{String(r.rolname).slice(0, 2)}</span>
                  <span class={cn("truncate font-mono text-ui-xs", isSel ? "font-medium text-foreground" : "text-muted-foreground")}>{r.rolname}</span>
                </button>
              {/each}
            {/each}
          </aside>

          <!-- Right: detail -->
          <div class="min-w-0 flex-1 overflow-y-auto">
            {#if !selectedRoleObj}
              <div class="flex h-full flex-col items-center justify-center gap-3 py-16 text-center">
                <Users class="size-9 shrink-0 text-muted-foreground" />
                <p class="font-mono text-ui-xs text-muted-foreground">Select a role to inspect its permissions</p>
              </div>
            {:else}
              {@const role = selectedRoleObj}
              <div class="flex items-center gap-2.5 border-b border-border/40 px-4 py-3">
                <span
                  class={cn(
                    "inline-flex size-7 shrink-0 items-center justify-center rounded-full font-mono text-ui-2xs font-semibold uppercase",
                    bool(role.rolsuper) ? "bg-destructive/10 text-destructive" : bool(role.rolcanlogin) ? "bg-primary/10 text-primary" : "bg-muted text-muted-foreground",
                  )}>{String(role.rolname).slice(0, 2)}</span>
                <div class="min-w-0">
                  <p class="truncate font-mono text-ui-sm font-medium text-foreground">{role.rolname}</p>
                  <p class="font-mono text-ui-3xs text-muted-foreground">
                    {bool(role.rolsuper) ? "Superuser" : bool(role.rolcanlogin) ? "Login role" : "Group role"}
                  </p>
                </div>
                <button
                  type="button"
                  class= "field-surface ml-auto inline-flex h-7 items-center gap-1.5 bg-background px-2.5 font-mono text-ui-xs text-muted-foreground transition-colors hover:bg-accent/35 hover:text-foreground"
                  onclick={() => openRoleModal(role)}
                ><KeyRound class="size-3.5 shrink-0" />Edit</button>
              </div>

              <!-- Role attributes -->
              <section class="border-b border-border/40 px-4 py-3">
                <p class="mb-2 font-mono text-ui-3xs uppercase tracking-[0.06em] text-muted-foreground">Role attributes</p>
                <div class="flex flex-wrap gap-1.5">
                  {#each ROLE_ATTRS as attr (attr.key)}
                    {@const on = bool(role[attr.key])}
                    <span
                      class={cn(
                        "inline-flex items-center gap-1 rounded-full px-2 py-0.5 font-mono text-ui-3xs font-medium",
                        on ? "bg-primary/10 text-primary" : "bg-muted/60 text-muted-foreground",
                      )}
                    >
                      {#if on}<Check class="size-3 shrink-0" />{/if}{attr.label}
                    </span>
                  {/each}
                  <span class="inline-flex items-center gap-1 rounded-full bg-muted/60 px-2 py-0.5 font-mono text-ui-3xs text-muted-foreground">
                    Conn limit: {role.rolconnlimit === -1 || role.rolconnlimit == null ? "∞" : role.rolconnlimit}
                  </span>
                  {#if role.rolvaliduntil}
                    <span class="inline-flex items-center gap-1 rounded-full bg-muted/60 px-2 py-0.5 font-mono text-ui-3xs text-muted-foreground">Expires: {role.rolvaliduntil}</span>
                  {/if}
                </div>
              </section>

              <!-- Membership -->
              <section class="border-b border-border/40 px-4 py-3">
                <p class="mb-2 font-mono text-ui-3xs uppercase tracking-[0.06em] text-muted-foreground">Inherited from · member of</p>
                {#if memberOf.length}
                  <div class="flex flex-wrap gap-1.5">
                    {#each memberOf as m (m)}
                      <button type="button" class="rounded-full bg-accent/40 px-2 py-0.5 font-mono text-ui-3xs text-foreground/80 transition-colors hover:bg-accent/60" onclick={() => selectRole(m)}>{m}</button>
                    {/each}
                  </div>
                {:else}
                  <p class="font-mono text-ui-2xs text-muted-foreground">Not a member of any role</p>
                {/if}
                {#if membersOfSelected.length}
                  <p class="mt-3 mb-2 font-mono text-ui-3xs uppercase tracking-[0.06em] text-muted-foreground">Granted to · members</p>
                  <div class="flex flex-wrap gap-1.5">
                    {#each membersOfSelected as m (m)}
                      <button type="button" class="rounded-full bg-muted/60 px-2 py-0.5 font-mono text-ui-3xs text-muted-foreground transition-colors hover:bg-accent/40" onclick={() => selectRole(m)}>{m}</button>
                    {/each}
                  </div>
                {/if}
              </section>

              <!-- Database access -->
              <section class="px-4 py-3">
                <p class="mb-2 font-mono text-ui-3xs uppercase tracking-[0.06em] text-muted-foreground">Database access</p>
                {#if dbGrantsLoading || dbGrants === null}
                  <div class="loading-reveal flex items-center gap-2 py-6 text-muted-foreground"><RefreshCw class="size-3.5 shrink-0 animate-spin" /><span class="font-mono text-ui-xs">Loading grants…</span></div>
                {:else if dbGrantsError}
                  <p class="font-mono text-ui-2xs text-destructive">{dbGrantsError}</p>
                {:else if dbGrants.length === 0}
                  <p class="font-mono text-ui-2xs text-muted-foreground">No databases</p>
                {:else}
                  <div class="overflow-hidden rounded-md border border-border/40">
                    <table class="w-full text-ui-xs">
                      <thead class="bg-panel text-left">
                        <tr class="border-b border-border/50">
                          <th class="px-3 py-1.5 font-mono font-normal text-muted-foreground">Database</th>
                          <th class="px-3 py-1.5 text-center font-mono font-normal text-muted-foreground" title="CONNECT">Connect</th>
                          <th class="px-3 py-1.5 text-center font-mono font-normal text-muted-foreground" title="CREATE">Create</th>
                          <th class="px-3 py-1.5 text-center font-mono font-normal text-muted-foreground" title="TEMP">Temp</th>
                        </tr>
                      </thead>
                      <tbody>
                        {#each dbGrants as g (g.datname)}
                          <tr class="border-b border-border/25 last:border-0 hover:bg-accent/20">
                            <td class="px-3 py-1.5 font-mono font-medium text-foreground">{g.datname}</td>
                            {#each ["can_connect", "can_create", "can_temp"] as col (col)}
                              <td class="px-3 py-1.5 text-center">
                                {#if bool(g[col])}
                                  <Check class="mx-auto size-3.5 shrink-0 text-primary" />
                                {:else}
                                  <span class="text-muted-foreground">—</span>
                                {/if}
                              </td>
                            {/each}
                          </tr>
                        {/each}
                      </tbody>
                    </table>
                  </div>
                {/if}
              </section>
            {/if}
          </div>
        </div>
      {/if}

      <!-- Policies -->
    {:else if activeTab === "policies"}
      <div
        class="flex items-center justify-between border-b border-border/40 px-3 py-2"
      >
        <span class="font-mono text-ui-xs text-muted-foreground"
          >{policies?.length ?? 0} polic{(policies?.length ?? 0) === 1
            ? "y"
            : "ies"}</span
        >
        <button
          type="button"
          class= "field-surface inline-flex h-7 items-center gap-1.5 bg-background px-2.5 font-mono text-ui-xs text-muted-foreground transition-colors hover:bg-accent/35 hover:text-foreground"
          onclick={openPolicyModal}
        >
          <Plus class="size-3.5 shrink-0" />New policy
        </button>
      </div>

      {#if policiesLoading || policies === null}
        <div
          class="loading-reveal flex items-center justify-center gap-2 py-16 text-muted-foreground"
        >
          <RefreshCw class="size-4 shrink-0 animate-spin" /><span
            class="font-mono text-ui-sm">Loading…</span
          >
        </div>
      {:else if policiesError}
        <div class="px-4 py-8 text-center">
          <p class="font-mono text-ui-xs text-destructive">{policiesError}</p>
          <button
            onclick={() => {
              policies = null;
              loadPolicies();
            }}
            class="mt-2 font-mono text-ui-xs text-muted-foreground underline"
            >Retry</button
          >
        </div>
      {:else if policies.length === 0}
        <div
          class="flex h-full flex-col items-center justify-center gap-3 py-20 text-center"
        >
          <ShieldCheck class="size-10 shrink-0 text-muted-foreground" />
          <p class="font-mono text-ui text-muted-foreground">
            No policies found
          </p>
          <p class="mt-1 text-ui-xs text-muted-foreground">
            Enable RLS on a table and create a policy to control row access
          </p>
          <button
            type="button"
            class= "field-surface mt-3 inline-flex h-7 items-center gap-1.5 bg-background px-2.5 font-mono text-ui-xs text-muted-foreground transition-colors hover:bg-accent/35 hover:text-foreground"
            onclick={openPolicyModal}
            ><Plus class="size-3.5 shrink-0" />Create first policy</button
          >
        </div>
      {:else}
        <table class="w-full text-ui-xs">
          <thead class="studio-chrome sticky top-0 z-10 bg-panel text-left">
            <tr class="border-b border-border/50">
              <th class="px-3 py-2 font-mono font-normal text-muted-foreground"
                >Table</th
              >
              <th class="px-3 py-2 font-mono font-normal text-muted-foreground"
                >Policy</th
              >
              <th class="px-3 py-2 font-mono font-normal text-muted-foreground"
                >Command</th
              >
              <th class="px-3 py-2 font-mono font-normal text-muted-foreground"
                >Roles</th
              >
              <th
                class="w-28 px-3 py-2 font-mono font-normal text-muted-foreground"
                >Type</th
              >
              <th class="w-9 px-2"></th>
            </tr>
          </thead>
          <tbody>
            {#each policies ?? [] as pol, i (i)}
              {@const permissive =
                (pol.permissive ?? "").toString().toUpperCase() ===
                "PERMISSIVE"}
              <tr
                class={cn(
                  "group/row border-b border-border/30 outline-none hover:bg-accent/25",
                  expandedPolicy === i && "bg-accent/15",
                )}
              >
                <td class="px-3 py-2 font-mono">
                  <span class="text-muted-foreground">{pol.schemaname}.</span
                  ><span class="text-foreground">{pol.tablename}</span>
                </td>
                <td class="px-3 py-2 font-mono text-foreground/80"
                  >{pol.policyname}</td
                >
                <td class="px-3 py-2">
                  <span
                    class="rounded bg-muted px-1.5 py-px font-mono text-ui-2xs text-muted-foreground"
                    >{pol.cmd}</span
                  >
                </td>
                <td
                  class="max-w-[100px] truncate px-3 py-2 font-mono text-muted-foreground"
                  >{pol.roles || "public"}</td
                >
                <td class="px-3 py-2">
                  <span
                    class={cn(
                      "rounded px-1.5 py-px font-mono text-ui-3xs font-medium",
                      permissive
                        ? "bg-primary/10 text-primary"
                        : "bg-destructive/10 text-destructive",
                    )}>{permissive ? "Permissive" : "Restrictive"}</span
                  >
                </td>
                <td class="px-2 py-2">
                  <button
                    type="button"
                    class="inline-flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                    onclick={() => {
                      expandedPolicy = expandedPolicy === i ? null : i;
                    }}
                    ><ChevronDown
                      class={cn(
                        "size-3.5 transition-transform",
                        expandedPolicy === i && "rotate-180",
                      )}
                    /></button
                  >
                </td>
              </tr>
              {#if expandedPolicy === i}
                <tr class="border-b border-border/30 bg-muted/20">
                  <td colspan="6" class="px-4 py-3">
                    <div
                      class={cn(
                        "grid gap-3",
                        pol.qual && pol.with_check
                          ? "grid-cols-2"
                          : "grid-cols-1",
                      )}
                    >
                      {#if pol.qual}
                        <div>
                          <p
                            class="mb-1.5 font-mono text-ui-2xs text-muted-foreground uppercase tracking-[0.06em]"
                          >
                            USING
                          </p>
                          <pre
                            class="rounded-md border border-border/40 bg-background px-3 py-2 font-mono text-ui-xs leading-relaxed text-foreground/90">{pol.qual}</pre>
                        </div>
                      {/if}
                      {#if pol.with_check}
                        <div>
                          <p
                            class="mb-1.5 font-mono text-ui-2xs text-muted-foreground uppercase tracking-[0.06em]"
                          >
                            WITH CHECK
                          </p>
                          <pre
                            class="rounded-md border border-border/40 bg-background px-3 py-2 font-mono text-ui-xs leading-relaxed text-foreground/90">{pol.with_check}</pre>
                        </div>
                      {/if}
                    </div>
                  </td>
                </tr>
              {/if}
            {/each}
          </tbody>
        </table>
      {/if}

      <!-- RLS -->
    {:else if activeTab === "rls"}
      {@const enabledCount = (rlsTables ?? []).filter((r) =>
        bool(r.rls_enabled),
      ).length}
      <div class="flex items-center gap-3 border-b border-border/40 px-3 py-2">
        <span class="font-mono text-ui-xs text-muted-foreground">
          {enabledCount} of {rlsTables?.length ?? 0} tables protected
        </span>
        {#if rlsTables && rlsTables.length > 0}
          <div class="h-1 flex-1 overflow-hidden rounded-full bg-muted">
            <div
              class="h-full rounded-full bg-primary/50 transition-[width]"
              style="width:{Math.round(
                (enabledCount / rlsTables.length) * 100,
              )}%"
            ></div>
          </div>
        {/if}
      </div>

      {#if rlsLoading || rlsTables === null}
        <div
          class="loading-reveal flex items-center justify-center gap-2 py-16 text-muted-foreground"
        >
          <RefreshCw class="size-4 shrink-0 animate-spin" /><span
            class="font-mono text-ui-sm">Loading…</span
          >
        </div>
      {:else if rlsError}
        <div class="px-4 py-8 text-center">
          <p class="font-mono text-ui-xs text-destructive">{rlsError}</p>
          <button
            onclick={() => {
              rlsTables = null;
              loadRls();
            }}
            class="mt-2 font-mono text-ui-xs text-muted-foreground underline"
            >Retry</button
          >
        </div>
      {:else}
        <table class="w-full text-ui-xs">
          <thead class="studio-chrome sticky top-0 z-10 bg-panel text-left">
            <tr class="border-b border-border/50">
              <th class="px-3 py-2 font-mono font-normal text-muted-foreground"
                >Schema</th
              >
              <th class="px-3 py-2 font-mono font-normal text-muted-foreground"
                >Table</th
              >
              <th
                class="px-3 py-2 text-center font-mono font-normal text-muted-foreground"
                >RLS</th
              >
              <th
                class="px-3 py-2 text-center font-mono font-normal text-muted-foreground"
                >Force RLS</th
              >
              <th class="w-28 px-3 py-2"></th>
            </tr>
          </thead>
          <tbody>
            {#each rlsTables ?? [] as row (`${row.schema}.${row.table_name}`)}
              {@const enabled = bool(row.rls_enabled)}
              {@const forced = bool(row.rls_forced)}
              <tr
                class="group/row border-b border-border/30 outline-none hover:bg-accent/25"
              >
                <td class="px-3 py-2 font-mono text-muted-foreground"
                  >{row.schema}</td
                >
                <td class="px-3 py-2 font-mono font-medium text-foreground"
                  >{row.table_name}</td
                >
                <td class="px-3 py-2 text-center">
                  {#if enabled}
                    <span
                      class="inline-flex items-center gap-1 rounded-full bg-primary/10 px-2 py-px font-mono text-ui-3xs font-medium text-primary"
                    >
                      <Lock class="size-3 shrink-0" />On
                    </span>
                  {:else}
                    <span
                      class="inline-flex items-center gap-1 rounded-full bg-muted px-2 py-px font-mono text-ui-3xs text-muted-foreground"
                    >
                      <LockOpen class="size-3 shrink-0" />Off
                    </span>
                  {/if}
                </td>
                <td class="px-3 py-2 text-center">
                  {#if forced}
                    <Check class="mx-auto size-3.5 shrink-0 text-primary" />
                  {:else}
                    <span class="text-muted-foreground">—</span>
                  {/if}
                </td>
                <td class="px-3 py-2">
                  <button
                    type="button"
                    class={cn(
                      "opacity-0 h-6 w-full rounded-md px-2 font-mono text-ui-2xs transition-colors group-hover/row:opacity-100",
                      enabled
                        ? "border border-border/50 text-muted-foreground hover:bg-accent/35 hover:text-foreground"
                        : "bg-primary/10 text-primary hover:bg-primary/15",
                    )}
                    onclick={() => openRlsModal(row)}
                    >{enabled ? "Disable" : "Enable"}</button
                  >
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    {/if}
  </div>
  {/if}
</div>

{#if sqlModal}
  <SecuritySqlModal
    title={sqlModal.title}
    bind:sql={sqlDraft}
    running={sqlRunning}
    onclose={() => {
      sqlModal = null;
    }}
    onrun={runSqlModal}
  />
{/if}
