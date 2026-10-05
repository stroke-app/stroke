/**
 * Cell values as text: CSV, Markdown, SQL literals and JSON for the grid's
 * copy and export, plus the type checks and comparison the grid's editing
 * uses. Pure functions, moved out of DataTable.svelte so they can be tested
 * on their own; the grid imports them.
 */
import { oversizeCellInfo, oversizeCellText } from '$lib/cell-value.js'

// Render a JS array as a Postgres array literal for display: {a,b}, {} for
// empty, NULL for null elements. Elements are quoted only when they contain a
// delimiter/quote/brace/whitespace or would be ambiguous - matching pgAdmin.
export function pgArrayElem(el) {
  if (el === null || el === undefined) return "NULL";
  // Nested arrays (multi-dim) recurse; objects (e.g. json[]) fall back to JSON.
  if (Array.isArray(el)) return pgArrayText(el);
  if (typeof el === "object") return JSON.stringify(el);
  const s = String(el);
  if (s === "" || /[",{}\\\s]/.test(s) || /^null$/i.test(s)) {
    return '"' + s.replace(/\\/g, "\\\\").replace(/"/g, '\\"') + '"';
  }
  return s;
}

export function pgArrayText(arr) {
  return "{" + arr.map(pgArrayElem).join(",") + "}";
}

/** True when a column's SQL type is an array (ends with []). */
export function isSqlArrayType(colType) {
  return /\[\]\s*$/.test(colType ?? "");
}

/** pgvector column types, whose values arrive as `[0.1,0.2,…]` text. */
export function isVectorType(colType) {
  return /^(vector|halfvec|sparsevec)\b/i.test(String(colType ?? "").trim());
}

/**
 * Fold a multi-line value onto the one line a grid row has for it.
 *
 * fillText draws no line breaks, so a newline came out as nothing at all
 * while the indentation around it was drawn in full - pretty-printed JSON
 * read as `[   "a",   "b" ]`, gaps where the structure used to be. The break
 * and the whitespace either side of it collapse to a single space, which is
 * what the copy-as-TSV path already does with the same values.
 */
export function foldLines(/** @type {string} */ s) {
  return s.includes("\n") || s.includes("\r") ? s.replace(/\s*[\r\n]+\s*/g, " ") : s;
}

/** Loose equality for cell values (handles object/array via JSON). */
export function valuesEqual(/** @type {unknown} */ a, /** @type {unknown} */ b) {
  if (a === b) return true;
  if (a === null || b === null || a === undefined || b === undefined) return false;
  if (typeof a === "object" || typeof b === "object") {
    try { const sa = JSON.stringify(a); return sa === JSON.stringify(b); } catch { return false; }
  }
  return false;
}

/** Full text of an object cell for copy/export - oversize sentinels become
 * their marker + preview so exports show the truncation explicitly. */
export function cellJsonString(value) {
  const over = oversizeCellInfo(value);
  return over ? oversizeCellText(over) : JSON.stringify(value);
}

/** Escape a cell value for CSV (RFC 4180). */
export function csvCell(value) {
  if (value === null || value === undefined) return '';
  const s = typeof value === 'object' ? cellJsonString(value) : String(value);
  if (s.includes(',') || s.includes('"') || s.includes('\n') || s.includes('\r')) {
    return '"' + s.replace(/"/g, '""') + '"';
  }
  return s;
}

/** Escape a cell value for SQL INSERT. */
export function cellSqlLiteral(value) {
  if (value === null || value === undefined) return 'NULL';
  if (typeof value === 'boolean') return value ? 'TRUE' : 'FALSE';
  if (typeof value === 'number') return String(value);
  if (typeof value === 'object') {
    const s = cellJsonString(value).replace(/'/g, "''");
    return `'${s}'`;
  }
  return "'" + String(value).replace(/'/g, "''") + "'";
}

/** Markdown-safe cell text. */
export function mdCell(value) {
  if (value === null || value === undefined) return 'NULL';
  const s = typeof value === 'object' ? cellJsonString(value) : String(value);
  return s.replace(/\|/g, '\\|').replace(/\n/g, ' ');
}

/** True when a cell holds the "this is N bytes" stand-in rather than a value. */
export function isOversizeValue(v) {
  return !!v && typeof v === 'object' && /** @type {any} */ (v).__strokeOversize === true
}
