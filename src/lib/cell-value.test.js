import { describe, it, expect } from "vitest";
import { asBooleanValue, formatTimestampForDb, parseCellInput } from "./cell-value.js";

describe("formatTimestampForDb", () => {
  it("keeps the instant when the value carries an explicit zone", () => {
    // The regression this file exists for. `Date.parse` accepts the ` UTC`
    // suffix chrono writes, so rebuilding the value from a local `Date` moved
    // it by the machine's offset and then dropped the offset entirely — every
    // edit of a timestamptz cell silently shifted the row.
    expect(formatTimestampForDb("2026-05-22 11:28:40.501 UTC")).toBe(
      "2026-05-22 11:28:40.501+00",
    );
    expect(formatTimestampForDb("2026-05-22T11:28:40.501Z")).toBe("2026-05-22 11:28:40.501+00");
    expect(formatTimestampForDb("2026-05-22 11:28:40 GMT")).toBe("2026-05-22 11:28:40+00");
  });

  it("preserves a non-UTC offset exactly as written", () => {
    expect(formatTimestampForDb("2026-05-22 11:28:40+05:45")).toBe("2026-05-22 11:28:40+05:45");
    expect(formatTimestampForDb("2026-05-22T11:28:40-0700")).toBe("2026-05-22 11:28:40-0700");
    expect(formatTimestampForDb("2026-05-22 11:28:40+00")).toBe("2026-05-22 11:28:40+00");
  });

  it("keeps fractional seconds", () => {
    // Dropping these spends a column's precision to save a value the user
    // never edited.
    expect(formatTimestampForDb("2026-05-22 11:28:40.501")).toBe("2026-05-22 11:28:40.501");
    expect(formatTimestampForDb("2026-05-22T11:28:40.123456")).toBe("2026-05-22 11:28:40.123456");
  });

  it("normalises the separator and fills in missing seconds", () => {
    expect(formatTimestampForDb("2026-05-22T11:28")).toBe("2026-05-22 11:28:00");
    expect(formatTimestampForDb("2026-05-22T11:28:40")).toBe("2026-05-22 11:28:40");
  });

  it("leaves a date-only value alone", () => {
    expect(formatTimestampForDb("2026-05-22")).toBe("2026-05-22");
  });

  it("is idempotent, so re-saving an untouched cell cannot drift", () => {
    for (const v of [
      "2026-05-22 11:28:40.501 UTC",
      "2026-05-22 11:28:40+05:45",
      "2026-05-22 11:28:40.501",
      "2026-05-22",
    ]) {
      const once = formatTimestampForDb(v);
      expect(formatTimestampForDb(once)).toBe(once);
    }
  });

  it("falls back to local interpretation only for text with no usable zone", () => {
    // A locale string has no offset to trust, so reading it as the user's own
    // wall clock is the right guess.
    const out = formatTimestampForDb("May 31, 2026 15:22:43");
    expect(out).toBe("2026-05-31 15:22:43");
  });

  it("passes unparseable text through for the database to reject", () => {
    // Better a clear "invalid input syntax" from Postgres than a value this
    // code invented.
    expect(formatTimestampForDb("2026-05-22 11:28:40 UTZ")).toBe("2026-05-22 11:28:40 UTZ");
  });

  it("returns empty input unchanged", () => {
    expect(formatTimestampForDb("")).toBe("");
    expect(formatTimestampForDb("   ")).toBe("");
  });
});

describe("parseCellInput — timestamps", () => {
  it("routes a timestamptz value through the normaliser", () => {
    expect(parseCellInput("2026-05-22 11:28:40.501 UTC", "timestamptz")).toEqual({
      ok: true,
      value: "2026-05-22 11:28:40.501+00",
    });
  });

  it("treats empty and NULL as null", () => {
    expect(parseCellInput("", "timestamptz")).toEqual({ ok: true, value: null });
    expect(parseCellInput("NULL", "timestamptz")).toEqual({ ok: true, value: null });
  });
});

describe('asBooleanValue', () => {
  it('reads 0/1 stored booleans as false/true', () => {
    expect(asBooleanValue(0)).toBe(false)
    expect(asBooleanValue(1)).toBe(true)
    expect(asBooleanValue('0')).toBe(false)
    expect(asBooleanValue('1')).toBe(true)
  })
  it('leaves real booleans, NULL and other values alone', () => {
    expect(asBooleanValue(true)).toBe(true)
    expect(asBooleanValue(null)).toBe(null)
    expect(asBooleanValue(2)).toBe(2)
    expect(asBooleanValue('yes')).toBe('yes')
  })
})
