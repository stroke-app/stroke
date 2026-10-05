import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import {
  PARAM_SCOPES_MAX,
  clearStoredParamValues,
  dialectForEngine,
  extractSqlParams,
  formatParamLiteral,
  loadScopedParamValues,
  missingSqlParams,
  saveScopedParamValues,
  substituteSqlParams,
} from "./sql-params.js";

const names = (/** @type {string} */ sql) => extractSqlParams(sql).map((p) => p.name);

describe("extractSqlParams", () => {
  it("finds parameters in first-appearance order, without duplicates", () => {
    expect(names("SELECT * FROM t WHERE a = :b AND c = :a AND d = :b")).toEqual(["b", "a"]);
  });

  it("ignores a Postgres :: cast", () => {
    expect(names("SELECT '1'::int, x::text FROM t")).toEqual([]);
  });

  it("ignores colons inside strings, identifiers and comments", () => {
    expect(names("SELECT ':nope' FROM t")).toEqual([]);
    expect(names('SELECT ":nope" FROM t')).toEqual([]);
    expect(names("SELECT 1 -- :nope\n")).toEqual([]);
    expect(names("SELECT 1 /* :nope */")).toEqual([]);
    expect(names("SELECT $$ :nope $$")).toEqual([]);
  });

  it("records every position of a repeated parameter", () => {
    const [p] = extractSqlParams("SELECT :x, :x");
    expect(p.positions).toHaveLength(2);
  });
});

describe("formatParamLiteral", () => {
  it("passes numbers, booleans and NULL through in auto mode", () => {
    expect(formatParamLiteral("42", "auto")).toBe("42");
    expect(formatParamLiteral("-1.5e3", "auto")).toBe("-1.5e3");
    expect(formatParamLiteral("true", "auto")).toBe("TRUE");
    expect(formatParamLiteral("null", "auto")).toBe("NULL");
  });

  it("quotes anything else", () => {
    expect(formatParamLiteral("Ada", "auto")).toBe("'Ada'");
    expect(formatParamLiteral("42", "text")).toBe("'42'");
  });

  it("emits NULL and raw text for their modes", () => {
    expect(formatParamLiteral("anything", "null")).toBe("NULL");
    expect(formatParamLiteral("now()", "raw")).toBe("now()");
  });

  it("doubles an embedded quote", () => {
    expect(formatParamLiteral("it's", "text")).toBe("'it''s'");
  });

  it("leaves a backslash alone under standard rules", () => {
    // In Postgres and SQLite a backslash is just a character. Escaping it here
    // would store two of them.
    expect(formatParamLiteral("a\\b", "text")).toBe("'a\\b'");
    expect(formatParamLiteral("trailing\\", "text")).toBe("'trailing\\'");
  });

  it("escapes a backslash for MySQL so a value cannot break out of its quotes", () => {
    expect(formatParamLiteral("a\\b", "text", "backslash")).toBe("'a\\\\b'");
    // The injection case: under MySQL rules the old output `'\'' OR 1=1 -- '`
    // closed the literal and ran the rest as SQL.
    const attack = "\\' OR 1=1 -- ";
    const literal = formatParamLiteral(attack, "text", "backslash");
    expect(literal).toBe("'\\\\'' OR 1=1 -- '");
    // Every backslash is paired, so none of them escapes a quote.
    const body = literal.slice(1, -1);
    expect(body.replace(/\\\\/g, "")).not.toContain("\\");
  });
});

describe("dialectForEngine", () => {
  it("only MySQL uses backslash escapes", () => {
    expect(dialectForEngine("mysql")).toBe("backslash");
    expect(dialectForEngine("postgres")).toBe("standard");
    expect(dialectForEngine("sqlite")).toBe("standard");
    expect(dialectForEngine(null)).toBe("standard");
  });
});

describe("missingSqlParams", () => {
  it("reports a parameter with no entry", () => {
    expect(missingSqlParams("SELECT :a", {}).map((p) => p.name)).toEqual(["a"]);
  });

  it("treats an explicit empty string and NULL as satisfied", () => {
    expect(missingSqlParams("SELECT :a", { a: { value: "", mode: "text" } })).toEqual([]);
    expect(missingSqlParams("SELECT :a", { a: { value: "", mode: "null" } })).toEqual([]);
  });

  it("reports a blank auto value", () => {
    expect(missingSqlParams("SELECT :a", { a: { value: "  ", mode: "auto" } })).toHaveLength(1);
  });
});

describe("substituteSqlParams", () => {
  it("replaces every occurrence", () => {
    const out = substituteSqlParams("SELECT :x, :x", { x: { value: "1", mode: "auto" } });
    expect(out).toBe("SELECT 1, 1");
  });

  it("leaves parameters without a value untouched", () => {
    expect(substituteSqlParams("SELECT :a, :b", { a: { value: "1", mode: "auto" } })).toBe(
      "SELECT 1, :b",
    );
  });

  it("does not touch a lookalike inside a string", () => {
    const sql = "SELECT ':x', :x";
    expect(substituteSqlParams(sql, { x: { value: "1", mode: "auto" } })).toBe("SELECT ':x', 1");
  });

  it("substitutes right-to-left so earlier positions stay valid", () => {
    // A long replacement for the first parameter would shift the second one's
    // offsets if the replacements ran forwards.
    const out = substituteSqlParams("SELECT :a, :b", {
      a: { value: "a-very-long-value", mode: "text" },
      b: { value: "2", mode: "auto" },
    });
    expect(out).toBe("SELECT 'a-very-long-value', 2");
  });

  it("uses the dialect it is given", () => {
    const values = { x: { value: "back\\slash", mode: /** @type {const} */ ("text") } };
    expect(substituteSqlParams("SELECT :x", values)).toBe("SELECT 'back\\slash'");
    expect(substituteSqlParams("SELECT :x", values, "backslash")).toBe("SELECT 'back\\\\slash'");
  });
});

describe("$name and ${name} variables", () => {
  it("finds both forms and records how each was written", () => {
    const ps = extractSqlParams("SELECT $name, ${limit_n} FROM users WHERE id = :id");
    expect(ps.map((p) => [p.name, p.sigil])).toEqual([["name", "$"], ["limit_n", "$"], ["id", ":"]]);
    expect(ps[1].positions[0]).toEqual({ start: 14, end: 24 });
  });

  it("treats :id and $id as one variable", () => {
    const [p, ...rest] = extractSqlParams("SELECT :id, $id, ${id}");
    expect(rest).toEqual([]);
    expect(p.positions).toHaveLength(3);
    expect(p.sigil).toBe(":");
  });

  it("leaves positional parameters alone", () => {
    expect(names("SELECT * FROM t WHERE a = $1 AND b = $2")).toEqual([]);
  });

  it("leaves dollar-quoted bodies alone", () => {
    expect(names("CREATE FUNCTION f() RETURNS int AS $$ SELECT $x $$ LANGUAGE sql")).toEqual([]);
    expect(names("DO $body$ BEGIN PERFORM $y; END $body$")).toEqual([]);
    // A variable after the body still counts.
    expect(names("SELECT $$ $nope $$, $yes")).toEqual(["yes"]);
  });

  it("leaves strings, quoted names and comments alone", () => {
    expect(names("SELECT '$nope', \"$nope\", `$nope` -- $nope\n/* ${nope} */")).toEqual([]);
  });

  it("leaves a $ inside a name alone", () => {
    expect(names("SELECT price$usd, t.col$2 FROM t")).toEqual([]);
  });

  it("does not take an unclosed brace", () => {
    expect(names("SELECT ${name FROM t")).toEqual([]);
  });

  it("does not confuse a :: cast after a variable", () => {
    expect(names("SELECT $when::date, :n::int")).toEqual(["when", "n"]);
  });

  it("leaves MySQL and SQL Server @variables alone", () => {
    expect(names("SET @x = 1; SELECT @x, @@version")).toEqual([]);
  });

  it("keeps SQL Server's own $ words out on SQL Server only", () => {
    const sql = "MERGE t USING s ON t.id = s.id WHEN MATCHED THEN DELETE OUTPUT $action, $identity;";
    expect(extractSqlParams(sql, { engine: "mssql" })).toEqual([]);
    expect(extractSqlParams(sql, { engine: "postgres" }).map((p) => p.name)).toEqual(["action", "identity"]);
  });

  it("substitutes every form, braces included", () => {
    const values = { name: { value: "Ada", mode: /** @type {const} */ ("auto") }, n: { value: "5", mode: /** @type {const} */ ("auto") } };
    expect(substituteSqlParams("SELECT * FROM u WHERE name = ${name} OR nick = $name LIMIT :n", values))
      .toBe("SELECT * FROM u WHERE name = 'Ada' OR nick = 'Ada' LIMIT 5");
  });

  it("reports a missing $ variable", () => {
    expect(missingSqlParams("SELECT $a, ${b}", { a: { value: "1", mode: "auto" } }).map((p) => p.name)).toEqual(["b"]);
  });
});

describe("remembered values per scope", () => {
  /** @type {Map<string, string>} */
  let store;
  beforeEach(() => {
    store = new Map();
    vi.stubGlobal("localStorage", {
      getItem: (/** @type {string} */ k) => (store.has(k) ? store.get(k) : null),
      setItem: (/** @type {string} */ k, /** @type {string} */ v) => void store.set(k, String(v)),
      removeItem: (/** @type {string} */ k) => void store.delete(k),
    });
  });
  afterEach(() => vi.unstubAllGlobals());

  it("keeps each scope's own values over the shared last-used ones", () => {
    saveScopedParamValues("conn|Query Editor", { id: { value: "1", mode: "auto" } });
    saveScopedParamValues("conn|Query Editor 2", { id: { value: "2", mode: "auto" } });
    expect(loadScopedParamValues("conn|Query Editor").id.value).toBe("1");
    expect(loadScopedParamValues("conn|Query Editor 2").id.value).toBe("2");
    // A tab with nothing of its own starts from the last value written anywhere.
    expect(loadScopedParamValues("conn|Query Editor 3").id.value).toBe("2");
  });

  it("drops the least recently written scopes past the cap", () => {
    const now = vi.spyOn(Date, "now");
    for (let i = 0; i <= PARAM_SCOPES_MAX; i++) {
      now.mockReturnValue(1000 + i);
      saveScopedParamValues(`s${i}`, { v: { value: String(i), mode: "auto" } });
    }
    now.mockRestore();
    const kept = JSON.parse(/** @type {string} */ (store.get("stroke:sql-param-values:scopes")));
    expect(Object.keys(kept)).toHaveLength(PARAM_SCOPES_MAX);
    expect(kept.s0).toBeUndefined();
    expect(kept[`s${PARAM_SCOPES_MAX}`]).toBeDefined();
  });

  it("forgets everything when cleared", () => {
    saveScopedParamValues("a", { x: { value: "1", mode: "auto" } });
    clearStoredParamValues();
    expect(loadScopedParamValues("a")).toEqual({});
  });
});
