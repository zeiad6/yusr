// Fails the build when ar/en drift apart or the code uses a key neither defines.
// Static keys: t("a.b"). Dynamic keys: t(`a.b.${x}`) must name an existing object `a.b`.
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

const root = new URL("../src/", import.meta.url).pathname;
const ar = JSON.parse(readFileSync(join(root, "i18n/ar.json"), "utf8"));
const en = JSON.parse(readFileSync(join(root, "i18n/en.json"), "utf8"));

function flatten(obj, prefix = "", out = new Map()) {
  for (const [k, v] of Object.entries(obj)) {
    const key = prefix ? `${prefix}.${k}` : k;
    out.set(key, v);
    if (v && typeof v === "object") flatten(v, key, out);
  }
  return out;
}

const arKeys = flatten(ar);
const enKeys = flatten(en);
const errors = [];
for (const k of arKeys.keys()) if (!enKeys.has(k)) errors.push(`en.json is missing ${k}`);
for (const k of enKeys.keys()) if (!arKeys.has(k)) errors.push(`ar.json is missing ${k}`);

function files(dir) {
  return readdirSync(dir).flatMap((f) => {
    const p = join(dir, f);
    return statSync(p).isDirectory() ? files(p) : /\.tsx?$/.test(f) ? [p] : [];
  });
}

for (const file of files(root)) {
  const src = readFileSync(file, "utf8");
  for (const [, key] of src.matchAll(/(?<![\w.])t\("([\w.]+)"/g)) {
    if (typeof arKeys.get(key) !== "string") errors.push(`${file.slice(root.length)}: unknown key "${key}"`);
  }
  for (const [, prefix] of src.matchAll(/(?<![\w.])t\(`([\w.]+)\.\$\{/g)) {
    const v = arKeys.get(prefix);
    if (!v || typeof v !== "object") errors.push(`${file.slice(root.length)}: unknown key group "${prefix}"`);
  }
}

if (errors.length) {
  console.error(errors.join("\n"));
  process.exit(1);
}
console.log(`i18n ok: ${arKeys.size} keys in ar/en`);
