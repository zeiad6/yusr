// Exact decimal arithmetic for amounts the UI adds up (journal balance check, totals).
// Floats would show 0.1 + 0.2 as unbalanced; BigInt at a fixed scale does not.

const SCALE = 4;
const FACTOR = 10n ** BigInt(SCALE);

/** Parses "1,234.5" / "-0.25" into units of 10^-SCALE; returns null if not a number. */
export function toUnits(value: string | number | null | undefined): bigint | null {
  const s = String(value ?? "").replace(/,/g, "").trim();
  const m = /^(-)?(\d*)(?:\.(\d*))?$/.exec(s);
  if (!m || (m[2] === "" && (m[3] ?? "") === "")) return null;
  const frac = (m[3] ?? "").slice(0, SCALE).padEnd(SCALE, "0");
  const units = BigInt(m[2] || "0") * FACTOR + BigInt(frac || "0");
  return m[1] ? -units : units;
}

export function fromUnits(units: bigint): string {
  const neg = units < 0n;
  const abs = neg ? -units : units;
  const int = abs / FACTOR;
  const frac = (abs % FACTOR).toString().padStart(SCALE, "0").replace(/0+$/, "");
  return `${neg ? "-" : ""}${int}${frac ? `.${frac}` : ""}`;
}

export function sum(values: (string | number | null | undefined)[]): string {
  return fromUnits(values.reduce<bigint>((acc, v) => acc + (toUnits(v) ?? 0n), 0n));
}

export function isPositive(value: string): boolean {
  const u = toUnits(value);
  return u !== null && u > 0n;
}

/** Display only: grouping and 2 decimals in the active locale. */
export function formatMoney(value: string | number | null | undefined, currency?: string, lang = "ar"): string {
  const n = Number(value ?? 0);
  const text = new Intl.NumberFormat(lang === "ar" ? "ar-SA-u-nu-latn" : "en-US", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  }).format(Number.isFinite(n) ? n : 0);
  return currency ? `${text} ${currency}` : text;
}

export function today(): string {
  const d = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}
