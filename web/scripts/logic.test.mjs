// Run: npm test. Termux's Node has no built-in TypeScript, so esbuild transpiles the
// dependency-free modules under test.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { transformSync } from "esbuild";

async function load(rel) {
  const ts = readFileSync(new URL(rel, import.meta.url), "utf8");
  const { code } = transformSync(ts, { loader: "ts", format: "esm" });
  return import(`data:text/javascript,${encodeURIComponent(code)}`);
}

const { toUnits, fromUnits, sum, isPositive } = await load("../src/utils/money.ts");
const { allows } = await load("../src/store/permissions.ts");

test("decimal parsing and exact sums", () => {
  assert.equal(toUnits("1,234.5"), 12345000n);
  assert.equal(toUnits("-0.25"), -2500n);
  assert.equal(toUnits(".5"), 5000n);
  assert.equal(toUnits("abc"), null);
  assert.equal(toUnits(""), null);
  assert.equal(sum(["0.1", "0.2"]), "0.3");
  assert.equal(sum(["100", "-100.00"]), "0");
  assert.equal(fromUnits(-12345n), "-1.2345");
  assert.ok(isPositive("0.01"));
  assert.ok(!isPositive("0"));
});

test("permission matrix matches crates/yusr-server/src/auth.rs", () => {
  assert.ok(allows("admin", "users.manage"));
  assert.ok(!allows("accountant", "users.manage"));
  assert.ok(!allows("accountant", "cases.manage"));
  assert.ok(allows("accountant", "ledger.post"));
  assert.ok(allows("case_worker", "aid.distribute"));
  assert.ok(!allows("case_worker", "ledger.read"));
  assert.ok(allows("auditor", "audit.read"));
  assert.ok(!allows("auditor", "ledger.post"));
  assert.ok(allows("viewer", "reports.read"));
  assert.ok(!allows("viewer", "donations.read"));
  assert.ok(!allows(undefined, "reports.read"));
  assert.ok(allows("accountant", "archive.write"));
  assert.ok(allows("case_worker", "archive.write"));
  assert.ok(allows("auditor", "archive.read"));
  assert.ok(!allows("auditor", "archive.write"));
  assert.ok(!allows("viewer", "archive.read"));
  assert.ok(allows("auditor", "archive.confidential"));
  assert.ok(allows("accountant", "archive.confidential"));
  assert.ok(!allows("case_worker", "archive.confidential"));
});
