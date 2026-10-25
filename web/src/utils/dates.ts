// Umm al-Qura dates from the browser's Intl data; the server's table is authoritative for
// stored records (journal entries, letters), this is for display and printed headers.

export function hijri(iso: string | Date, lang: string, style: "long" | "numeric" = "long"): string {
  const date = typeof iso === "string" ? new Date(`${iso.slice(0, 10)}T12:00:00`) : iso;
  const locale = lang.startsWith("ar") ? "ar-SA-u-ca-islamic-umalqura-nu-latn" : "en-US-u-ca-islamic-umalqura";
  const opts: Intl.DateTimeFormatOptions = style === "long" ? { day: "numeric", month: "long", year: "numeric" } : { day: "2-digit", month: "2-digit", year: "numeric" };
  try {
    return new Intl.DateTimeFormat(locale, opts).format(date);
  } catch {
    return "";
  }
}

export function gregorian(iso: string | Date, lang: string): string {
  const date = typeof iso === "string" ? new Date(`${iso.slice(0, 10)}T12:00:00`) : iso;
  return new Intl.DateTimeFormat(lang.startsWith("ar") ? "ar-SA-u-ca-gregory-nu-latn" : "en-GB", { day: "numeric", month: "long", year: "numeric" }).format(date);
}

/** Server `YYYY-MM-DD` Hijri string, e.g. "1448-05-14", shown as 14/05/1448 هـ. */
export function hijriText(ymd: string, lang: string): string {
  const [y, m, d] = ymd.split("-");
  return lang.startsWith("ar") ? `${d}/${m}/${y} هـ` : `${d}/${m}/${y} AH`;
}

export function humanSize(bytes: number, lang: string): string {
  const units = lang.startsWith("ar") ? ["بايت", "ك.ب", "م.ب", "ج.ب"] : ["B", "KB", "MB", "GB"];
  let v = bytes;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
}
