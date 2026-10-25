// One renderer for every printable document (reports, letters, archive cards): it builds a
// self-contained A4 HTML page that is shown in an <iframe> preview, printed through a hidden
// iframe (print or "Save as PDF") and saved as a Word file. Everything user-supplied goes
// through `esc`, because the page is same-origin.
import ar400 from "@fontsource/ibm-plex-sans-arabic/files/ibm-plex-sans-arabic-arabic-400-normal.woff2?url";
import ar600 from "@fontsource/ibm-plex-sans-arabic/files/ibm-plex-sans-arabic-arabic-600-normal.woff2?url";
import ar700 from "@fontsource/ibm-plex-sans-arabic/files/ibm-plex-sans-arabic-arabic-700-normal.woff2?url";
import la400 from "@fontsource/ibm-plex-sans-arabic/files/ibm-plex-sans-arabic-latin-400-normal.woff2?url";
import la600 from "@fontsource/ibm-plex-sans-arabic/files/ibm-plex-sans-arabic-latin-600-normal.woff2?url";
import la700 from "@fontsource/ibm-plex-sans-arabic/files/ibm-plex-sans-arabic-latin-700-normal.woff2?url";
import type { Organization } from "../api/types";
import { download } from "../api/client";

export function esc(v: unknown): string {
  return String(v ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]!);
}

/** Text with blank-line paragraphs and single line breaks, escaped. */
export function paragraphs(text: string): string {
  return text
    .trim()
    .split(/\n\s*\n/)
    .map((p) => `<p>${esc(p).replace(/\n/g, "<br>")}</p>`)
    .join("");
}

const ARABIC = "U+0600-06FF,U+0750-077F,U+0870-088E,U+0890-0891,U+0897-08E1,U+08E3-08FF,U+200C-200E,U+2010-2011,U+204F,U+2E41,U+FB50-FDFF,U+FE70-FE74,U+FE76-FEFC";
const LATIN = "U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+0304,U+0308,U+0329,U+2000-206F,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,U+FFFD";

function fontFaces(): string {
  const face = (url: string, weight: number, range: string) =>
    `@font-face{font-family:"Plex Arabic";src:url(${new URL(url, location.href).href}) format("woff2");font-weight:${weight};unicode-range:${range};font-display:block}`;
  return [face(ar400, 400, ARABIC), face(ar600, 600, ARABIC), face(ar700, 700, ARABIC), face(la400, 400, LATIN), face(la600, 600, LATIN), face(la700, 700, LATIN)].join("");
}

/** A4 with 18/20 mm top/bottom and 15 mm side margins: room for binding and a running page number,
 *  worded per language because "2 / 1" reads ambiguously in right-to-left text. */
function pageRule(lang: string): string {
  const label = lang.startsWith("ar") ? '"صفحة " counter(page) " من " counter(pages)' : '"Page " counter(page) " of " counter(pages)';
  return `@page{size:A4;margin:18mm 15mm 20mm 15mm;@bottom-center{content:${label};font:8pt "Plex Arabic",sans-serif;color:#7a8584}}`;
}

const PAGE_CSS = `
*{box-sizing:border-box}
html{-webkit-print-color-adjust:exact;print-color-adjust:exact}
body{margin:0;font-family:"Plex Arabic","Noto Naskh Arabic",Tahoma,sans-serif;font-size:10.5pt;line-height:1.65;color:#1d2a2a;background:#e9e6de}
.sheet{width:210mm;min-height:297mm;margin:10mm auto;padding:18mm 15mm 20mm;background:#fff;box-shadow:0 2px 14px rgba(20,40,40,.18);position:relative}
@media screen{html{overflow-x:hidden}}
@media print{body{background:#fff}.sheet{width:auto;min-height:0;margin:0;padding:0;box-shadow:none;zoom:1!important}}
.lh{display:flex;align-items:center;justify-content:space-between;gap:12mm;padding-bottom:4mm;border-bottom:1.2pt solid #0f766e;position:relative;margin-bottom:7mm}
.lh::after{content:"";position:absolute;inset-inline:0;bottom:-3pt;border-bottom:.6pt solid #a98336}
.lh .org{flex:1}.lh .org.en{direction:ltr;text-align:left}
.lh .name{font-size:14pt;font-weight:700;color:#0e2a2b;line-height:1.3}
.lh .sub{font-size:8.5pt;color:#5c6968}
.lh img{max-height:22mm;max-width:40mm;object-fit:contain}
.lh .mark{width:16mm;height:16mm}
.lh-contact{margin:-5mm 0 6mm;text-align:center;font-size:8pt;color:#5c6968;direction:ltr;unicode-bidi:plaintext}
h1.title{font-size:16pt;margin:0 0 1mm;color:#0e2a2b}
.subtitle{color:#5c6968;font-size:9.5pt;margin-bottom:5mm}
.meta{display:grid;grid-template-columns:repeat(3,1fr);gap:2mm 6mm;font-size:9pt;margin:0 0 6mm;padding:3mm 4mm;background:#f7f6f1;border:.6pt solid #e6e2d8;border-radius:2mm}
.meta b{display:block;color:#5c6968;font-weight:600;font-size:8pt}
.kpis{display:grid;grid-template-columns:repeat(4,1fr);gap:3mm;margin:0 0 6mm}
.kpi{border:.6pt solid #e6e2d8;border-top:2pt solid #0f766e;border-radius:1.5mm;padding:2.5mm 3mm}
.kpi:nth-child(2){border-top-color:#a98336}
.kpi .l{font-size:8pt;color:#5c6968}.kpi .v{font-size:13pt;font-weight:700;font-feature-settings:"tnum"}
h2.sec{font-size:11pt;margin:6mm 0 2mm;color:#0f766e;break-after:avoid}
table.t{width:100%;border-collapse:collapse;font-size:9pt;margin-bottom:3mm}
table.t thead{display:table-header-group}
table.t th{background:#0e2a2b;color:#fff;font-weight:600;padding:1.6mm 2mm;text-align:start}
table.t td{padding:1.4mm 2mm;border-bottom:.5pt solid #e6e2d8;vertical-align:top}
table.t tbody tr:nth-child(even) td{background:#faf9f5}
table.t tr{break-inside:avoid}
table.t .n{text-align:end;font-feature-settings:"tnum";white-space:nowrap}
table.t tr.total td{font-weight:700;border-top:1pt solid #0e2a2b;border-bottom:1.5pt double #0e2a2b;background:#f5eddb}
table.t tr.group td{font-weight:700;background:#e5f2ef;color:#0d6a62}
.empty{padding:6mm;text-align:center;color:#8a9392;border:.6pt dashed #d6d0c2;border-radius:2mm}
.sign{display:grid;grid-template-columns:repeat(3,1fr);gap:10mm;margin-top:14mm;break-inside:avoid}
.sign div{text-align:center;font-size:9pt}.sign span{display:block;border-top:.6pt solid #1d2a2a;margin-top:14mm;padding-top:1.5mm;color:#5c6968}
.foot{margin-top:8mm;padding-top:2mm;border-top:.5pt solid #e6e2d8;font-size:7.5pt;color:#8a9392;display:flex;justify-content:space-between;gap:4mm}
.ltr{direction:ltr;unicode-bidi:isolate}
`;

export interface PaperDoc {
  title: string;
  lang: string;
  org: Organization;
  /** Inner HTML placed after the letterhead; build it with `esc`. */
  body: string;
  extraCss?: string;
  /** Footer left/right text, escaped here. */
  footer?: [string, string];
}

function letterhead(org: Organization, lang: string): string {
  const mark = org.logo_data_url
    ? `<img src="${esc(org.logo_data_url)}" alt="">`
    : `<svg class="mark" viewBox="0 0 64 64"><defs><linearGradient id="ym-bg" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#14837b"/><stop offset="1" stop-color="#083f3c"/></linearGradient><linearGradient id="ym-gold" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#f6dc96"/><stop offset="1" stop-color="#c0902f"/></linearGradient></defs><rect width="64" height="64" rx="14.72" fill="url(#ym-bg)"/><rect x=".75" y=".75" width="62.5" height="62.5" rx="14.08" fill="none" stroke="#fff" stroke-opacity=".09" stroke-width="1.5"/><g fill="none" stroke="url(#ym-gold)" stroke-width="1.79" stroke-linejoin="round" opacity=".55"><rect x="12.80" y="12.80" width="38.40" height="38.40" rx="1.54"/><rect x="12.80" y="12.80" width="38.40" height="38.40" rx="1.54" transform="rotate(45 32.0 32.0)"/></g><path d="M174 0V140H320V0ZM-85 -150Q-41 -162 -12.5 -159.0Q16 -156 32.0 -142.0Q48 -128 54.0 -106.5Q60 -85 60 -60V309L200 340V-60Q200 -113 177.5 -146.5Q155 -180 119.0 -195.5Q83 -211 43.5 -210.0Q4 -209 -30.5 -193.5Q-65 -178 -85 -150ZM280 0V140H360V309L500 340V140H560V359L700 390V140H760V409L900 440V140H1020V0ZM1138 740Q1108 723 1073.5 732.5Q1039 742 1021 773Q1003 805 1011.5 840.0Q1020 875 1050 892Q1082 911 1117.5 901.5Q1153 892 1172 860Q1189 829 1179.5 793.5Q1170 758 1138 740ZM990 654 973 684 1123 770 1138 740ZM1123 770Q1143 781 1148.0 802.5Q1153 824 1142 843Q1131 863 1110.5 868.5Q1090 874 1069 863Q1050 853 1045.5 831.5Q1041 810 1051 790Q1062 771 1083.0 765.0Q1104 759 1123 770ZM1169 -208Q1145 -208 1128.0 -191.0Q1111 -174 1111 -150Q1111 -126 1128.0 -109.0Q1145 -92 1169 -92Q1193 -92 1210.0 -109.0Q1227 -126 1227 -150Q1227 -174 1210.0 -191.0Q1193 -208 1169 -208ZM996 -208Q972 -208 955.0 -191.0Q938 -174 938 -150Q938 -126 955.0 -109.0Q972 -92 996 -92Q1020 -92 1037.0 -109.0Q1054 -126 1054 -150Q1054 -174 1037.0 -191.0Q1020 -208 996 -208ZM980 0V140H1060V309L1200 340V23Q1200 11 1190.0 6.0Q1180 1 1170.0 0.5Q1160 0 1160 0Z" transform="matrix(0.02732 0 0 -0.02732 16.402 42.127)" fill="url(#ym-gold)"/></svg>`;
  const line = (parts: (string | false | null | undefined)[]) => parts.filter(Boolean).map(esc).join(" · ");
  const ar = `<div class="org"><div class="name">${esc(org.name_ar)}</div><div class="sub">${line([org.license_no && `ترخيص رقم ${org.license_no}`, org.vat_number && `الرقم الضريبي ${org.vat_number}`])}</div></div>`;
  const en = `<div class="org en"><div class="name">${esc(org.name_en)}</div><div class="sub">${line([org.license_no && `Licence ${org.license_no}`, org.address])}</div></div>`;
  const contact = line([org.registration_no && (lang.startsWith("ar") ? `رقم التسجيل ${org.registration_no}` : `Reg. ${org.registration_no}`), org.phone, org.email, org.website]);
  return `<header class="lh">${lang.startsWith("ar") ? ar : en}${mark}${lang.startsWith("ar") ? en : ar}</header>${contact ? `<div class="lh-contact">${contact}</div>` : ""}`;
}

/** Full standalone HTML document. */
export function paperHtml(d: PaperDoc): string {
  const dir = d.lang.startsWith("ar") ? "rtl" : "ltr";
  const foot = d.footer ? `<footer class="foot"><span>${esc(d.footer[0])}</span><span>${esc(d.footer[1])}</span></footer>` : "";
  return `<!doctype html><html lang="${d.lang}" dir="${dir}"><head><meta charset="utf-8"><title>${esc(d.title)}</title><style>${fontFaces()}${pageRule(d.lang)}${PAGE_CSS}${d.extraCss ?? ""}</style></head><body><div class="sheet">${letterhead(d.org, d.lang)}${d.body}${foot}</div></body></html>`;
}

/** Shrinks the A4 sheet to fit a preview frame on screen; printing resets the zoom in CSS.
 *  Done from the parent page because the CSP forbids inline script inside the frame. */
export function fitPaper(frame: HTMLIFrameElement) {
  const doc = frame.contentDocument;
  const sheet = doc?.querySelector<HTMLElement>(".sheet");
  if (!doc || !sheet) return;
  sheet.style.zoom = "";
  const z = Math.min(1, (doc.documentElement.clientWidth - 24) / sheet.offsetWidth);
  if (z < 1) sheet.style.zoom = String(z);
}

/** Prints through a hidden iframe so only the document goes to the printer or PDF. */
export function printHtml(html: string): Promise<void> {
  return new Promise((resolve) => {
    const frame = document.createElement("iframe");
    frame.setAttribute("aria-hidden", "true");
    frame.style.cssText = "position:fixed;inset-inline-start:-10000px;top:0;width:210mm;height:297mm;border:0";
    frame.srcdoc = html;
    frame.onload = async () => {
      const w = frame.contentWindow!;
      try {
        await w.document.fonts.ready;
      } catch {
        // fonts API unavailable: print with whatever is loaded
      }
      const cleanup = () => {
        frame.remove();
        resolve();
      };
      w.addEventListener("afterprint", () => setTimeout(cleanup, 500), { once: true });
      w.focus();
      w.print();
      // Some browsers never fire afterprint; never leave the frame behind.
      setTimeout(cleanup, 120_000);
    };
    document.body.appendChild(frame);
  });
}

/** Word opens HTML saved as .doc; the mso @page rule keeps A4 and the margins. */
export function downloadWord(html: string, fileName: string) {
  const word = html
    .replace(/<script[\s\S]*?<\/script>/g, "")
    .replace("<html ", '<html xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:w="urn:schemas-microsoft-com:office:word" ')
    .replace("</style>", "@page WordSection{size:21cm 29.7cm;margin:1.8cm 1.5cm 2cm 1.5cm}.sheet{page:WordSection;box-shadow:none;margin:0;width:auto;padding:0}body{background:#fff}</style>");
  download(`${fileName}.doc`, "﻿" + word, "application/msword");
}

export interface CsvSection {
  heading: string;
  columns: string[];
  rows: (string | number)[][];
}

export function downloadCsv(fileName: string, sections: CsvSection[]) {
  const cell = (v: string | number) => {
    const s = String(v ?? "");
    return /[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
  };
  const lines: string[] = [];
  for (const s of sections) {
    if (lines.length) lines.push("");
    lines.push(cell(s.heading));
    lines.push(s.columns.map(cell).join(","));
    for (const r of s.rows) lines.push(r.map(cell).join(","));
  }
  // BOM so Excel reads Arabic as UTF-8.
  download(`${fileName}.csv`, "﻿" + lines.join("\n"), "text/csv;charset=utf-8");
}
