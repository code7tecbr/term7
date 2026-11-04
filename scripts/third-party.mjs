#!/usr/bin/env node
// Gera o THIRD_PARTY_LICENSES.md que vai dentro do .deb: as licenças MIT/Apache/BSD
// das dependências pedem que o aviso de copyright acompanhe o programa distribuído.
// Inclui só o que entra no app: pacotes npm de produção e crates do alvo Linux.
//
// Uso: node scripts/third-party.mjs > src-tauri/THIRD_PARTY_LICENSES.md
// (precisa do node_modules instalado e do cargo; roda no build, antes do bundle).

import { execFileSync } from "node:child_process";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const LICENSE_FILE = /^(licen[cs]e|copying|notice)([-._].*)?$/i;

/** Textos de licença que vieram no pacote (LICENSE, LICENSE-MIT, NOTICE...). */
function licenseTexts(dir) {
  let names = [];
  try {
    names = readdirSync(dir).filter((n) => LICENSE_FILE.test(n)).sort();
  } catch {
    return [];
  }
  return names.map((n) => ({ name: n, text: readFileSync(join(dir, n), "utf8").trim() }));
}

function npmPackages() {
  const lock = JSON.parse(readFileSync(join(root, "package-lock.json"), "utf8")).packages;
  return Object.entries(lock)
    .filter(([path, p]) => path && !p.dev)
    .map(([path, p]) => {
      const dir = join(root, path);
      const manifest = JSON.parse(readFileSync(join(dir, "package.json"), "utf8"));
      const license = typeof manifest.license === "object" ? manifest.license.type : manifest.license;
      return { name: manifest.name, version: p.version, license: license ?? "?", files: licenseTexts(dir) };
    });
}

function cratePackages() {
  const out = execFileSync(
    "cargo",
    ["metadata", "--format-version", "1", "--locked", "--filter-platform", "x86_64-unknown-linux-gnu"],
    { cwd: join(root, "src-tauri"), maxBuffer: 256 << 20 },
  );
  const meta = JSON.parse(out.toString());
  const byId = new Map(meta.packages.map((p) => [p.id, p]));
  const nodes = new Map(meta.resolve.nodes.map((n) => [n.id, n]));
  // Só o que o app usa de verdade: dependências normais a partir da raiz (sem dev/build-only de teste).
  const seen = new Set();
  const stack = [meta.resolve.root];
  while (stack.length) {
    const id = stack.pop();
    if (seen.has(id)) continue;
    seen.add(id);
    for (const dep of nodes.get(id)?.deps ?? []) {
      if (dep.dep_kinds.some((k) => k.kind !== "dev")) stack.push(dep.pkg);
    }
  }
  return [...seen]
    .map((id) => byId.get(id))
    .filter((p) => p && p.source)
    .map((p) => ({
      name: p.name,
      version: p.version,
      license: p.license ?? "?",
      files: licenseTexts(dirname(p.manifest_path)),
    }));
}

function section(title, packages, home) {
  const list = packages.sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));
  const lines = [`## ${title}`, ""];
  for (const p of list) {
    lines.push(`### ${p.name} ${p.version}`, "", `License: ${p.license}`, "");
    if (!p.files.length) lines.push(`The package ships no license file; see ${home(p)}.`, "");
    for (const f of p.files) lines.push(`<details><summary>${f.name}</summary>`, "", "```text", f.text, "```", "", "</details>", "");
  }
  return lines.join("\n");
}

const fontNotice = readFileSync(join(root, "src/assets/term7-anon.LICENSE.txt"), "utf8").trim();

process.stdout.write(
  [
    "# Third-party licenses",
    "",
    "term7 is MIT licensed (see LICENSE). It ships with the open source software listed below.",
    "",
    "## Fonts",
    "",
    "### term7 Anon (derived from DejaVu Sans Mono)",
    "",
    "```text",
    fontNotice,
    "```",
    "",
    section("npm packages", npmPackages(), (p) => `https://www.npmjs.com/package/${p.name}/v/${p.version}`),
    section("Rust crates", cratePackages(), (p) => `https://crates.io/crates/${p.name}/${p.version}`),
  ].join("\n") + "\n",
);
