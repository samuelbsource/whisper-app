// Generates THIRD-PARTY-NOTICES.txt: the licence texts of everything that ends up inside the app.
//
//   node scripts/notices.mjs           write the file
//   node scripts/notices.mjs --check   fail if the committed file is out of date (used by CI)
//
// Rust: normal (non-build, non-dev) dependencies of the app, from `cargo metadata`.
// npm:  svelte (its runtime is bundled into the UI) and the @tauri-apps packages, plus their dependencies.
// The file is embedded into the app (see src-tauri/src/commands.rs) and shown in Settings > About.

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const out = join(root, "THIRD-PARTY-NOTICES.txt");
const check = process.argv.includes("--check");

const LICENSE_FILE = /^(licen[cs]e|copying|notice|unlicense)([-._].*)?$/i;

function licenseFiles(dir) {
  if (!existsSync(dir)) return [];
  return readdirSync(dir, { withFileTypes: true })
    .filter((e) => e.isFile() && LICENSE_FILE.test(e.name))
    .map((e) => e.name)
    .sort();
}

const norm = (text) => text.replace(/\r\n?/g, "\n").trim();

/** Collect { name, version, license, repository, texts[] } for every shipped package. */
const entries = [];

// ---- Rust -------------------------------------------------------------------------------------
const meta = JSON.parse(
  execFileSync(
    "cargo",
    ["metadata", "--format-version", "1", "--locked", "--all-features", "--filter-platform", "x86_64-pc-windows-msvc"],
    { cwd: join(root, "src-tauri"), maxBuffer: 1 << 29, encoding: "utf8" },
  ),
);
const byId = new Map(meta.packages.map((p) => [p.id, p]));
const nodes = new Map(meta.resolve.nodes.map((n) => [n.id, n]));
const seen = new Set();
const stack = [meta.resolve.root];
while (stack.length) {
  const id = stack.pop();
  if (seen.has(id)) continue;
  seen.add(id);
  for (const dep of nodes.get(id).deps) {
    // kind === null is a normal dependency; "build" and "dev" never ship in the binary.
    if (dep.dep_kinds.some((k) => k.kind === null)) stack.push(dep.pkg);
  }
}
for (const id of seen) {
  if (id === meta.resolve.root) continue;
  const pkg = byId.get(id);
  const dir = dirname(pkg.manifest_path);
  const texts = licenseFiles(dir).map((f) => ({ file: f, text: norm(readFileSync(join(dir, f), "utf8")) }));
  entries.push({
    kind: "rust",
    name: pkg.name,
    version: pkg.version,
    license: pkg.license ?? (pkg.license_file ? `see ${pkg.license_file}` : "unknown"),
    repository: pkg.repository ?? "",
    texts,
  });
  // whisper.cpp itself is vendored inside whisper-rs-sys.
  if (pkg.name === "whisper-rs-sys") {
    const wc = join(dir, "whisper.cpp");
    const texts = licenseFiles(wc).map((f) => ({ file: f, text: norm(readFileSync(join(wc, f), "utf8")) }));
    entries.push({
      kind: "native",
      name: "whisper.cpp (incl. ggml)",
      version: "bundled with whisper-rs-sys " + pkg.version,
      license: "MIT",
      repository: "https://github.com/ggml-org/whisper.cpp",
      texts,
    });
  }
}

// ---- npm --------------------------------------------------------------------------------------
const lock = JSON.parse(readFileSync(join(root, "package-lock.json"), "utf8")).packages;
const npmSeen = new Set();
function addNpm(name) {
  const key = `node_modules/${name}`;
  if (npmSeen.has(key) || !lock[key]) return;
  npmSeen.add(key);
  const info = lock[key];
  const dir = join(root, key);
  const texts = licenseFiles(dir).map((f) => ({ file: f, text: norm(readFileSync(join(dir, f), "utf8")) }));
  entries.push({
    kind: "npm",
    name,
    version: info.version,
    license: info.license ?? "unknown",
    repository: "",
    texts,
  });
  for (const dep of Object.keys(info.dependencies ?? {})) addNpm(dep);
}
for (const name of ["svelte", "@tauri-apps/api", "@tauri-apps/plugin-dialog"]) addNpm(name);

// ---- Group identical licence texts ------------------------------------------------------------
entries.sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));
const groups = new Map(); // hash -> { text, users[] }
const noText = [];
for (const e of entries) {
  if (!e.texts.length) {
    noText.push(e);
    continue;
  }
  for (const { text } of e.texts) {
    const hash = createHash("sha256").update(text).digest("hex");
    if (!groups.has(hash)) groups.set(hash, { text, users: [] });
    const g = groups.get(hash);
    if (!g.users.includes(e)) g.users.push(e);
  }
}

const label = (e) => `${e.name} ${e.version} (${e.license})${e.repository ? " " + e.repository : ""}`;
const rule = "=".repeat(78);
const lines = [];
lines.push("THIRD-PARTY NOTICES");
lines.push("");
lines.push("Win Whisper itself is released under the MIT licence (see LICENSE). It includes the software");
lines.push("below, each under its own licence. Identical licence texts are listed once, followed by every");
lines.push("package that uses them. Generated by scripts/notices.mjs; do not edit by hand.");
lines.push("");
lines.push(`Packages: ${entries.length} (${entries.filter((e) => e.kind === "rust").length} Rust crates, ` +
  `${entries.filter((e) => e.kind === "npm").length} npm packages, ` +
  `${entries.filter((e) => e.kind === "native").length} bundled native library)`);

const sorted = [...groups.values()].sort((a, b) => label(a.users[0]).localeCompare(label(b.users[0])));
for (const g of sorted) {
  lines.push("", rule, "Used by:");
  for (const u of g.users) lines.push("  - " + label(u));
  lines.push(rule, "", g.text);
}
if (noText.length) {
  lines.push("", rule, "Packages that do not ship a licence file (licence as declared in their metadata):", rule);
  for (const e of noText) lines.push("  - " + label(e));
}
const result = lines.join("\n") + "\n";

if (check) {
  const current = existsSync(out) ? readFileSync(out, "utf8").replace(/\r\n/g, "\n") : "";
  if (current !== result) {
    console.error("THIRD-PARTY-NOTICES.txt is out of date. Run: node scripts/notices.mjs");
    process.exit(1);
  }
  console.log("THIRD-PARTY-NOTICES.txt is up to date.");
} else {
  writeFileSync(out, result);
  console.log(`Wrote ${out}: ${entries.length} packages, ${groups.size} distinct licence texts, ${(result.length / 1024).toFixed(0)} KB`);
  if (noText.length) console.log(`${noText.length} packages ship no licence file: ${noText.map((e) => e.name).join(", ")}`);
}
