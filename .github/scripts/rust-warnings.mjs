// Avisos do `cargo audit --json` que o Dependabot não cobre: crates sem manutenção,
// inseguras (unsound) ou retiradas (yanked). Imprime Markdown pra issue; sem nenhum, nada.
import { readFileSync } from "node:fs";

const [auditFile = "audit.json", manifest = "src-tauri/Cargo.toml"] = process.argv.slice(2);
const audit = JSON.parse(readFileSync(auditFile, "utf8"));

// Nomes em [dependencies], [build-dependencies] etc. do Cargo.toml do app.
const direct = new Set();
let inDeps = false;
for (const line of readFileSync(manifest, "utf8").split("\n")) {
  const section = /^\s*\[([^\]]+)\]/.exec(line);
  if (section) inDeps = /(^|\.)(dev-|build-)?dependencies$/.test(section[1]);
  else if (inDeps) {
    const dep = /^\s*([A-Za-z0-9_-]+)\s*=/.exec(line);
    if (dep) direct.add(dep[1]);
  }
}

const KINDS = { unmaintained: "sem manutenção", unsound: "código inseguro (unsound)", yanked: "versão retirada (yanked)" };
const rows = [];
for (const [kind, warnings] of Object.entries(audit.warnings ?? {})) {
  if (!KINDS[kind]) continue;
  for (const w of warnings) {
    const adv = w.advisory;
    const link = adv ? `[${adv.id}](https://rustsec.org/advisories/${adv.id})` : "";
    const what = [adv?.title, link].filter(Boolean).join(" — ").replace(/\|/g, "\\|");
    rows.push(`| \`${w.package.name}@${w.package.version}\` | ${direct.has(w.package.name) ? "direto" : "dependência"} | ${KINDS[kind]} | ${what} |`);
  }
}

if (rows.length) {
  console.log(
    [
      "## Rust (Cargo)",
      "",
      "Avisos do `cargo audit` no `src-tauri/Cargo.lock`. As de \"dependência\" vêm de outras crates (quase sempre do Tauri) e saem quando elas atualizarem.",
      "",
      "| Crate | Onde | Aviso | Detalhe |",
      "| --- | --- | --- | --- |",
      ...[...new Set(rows)],
    ].join("\n"),
  );
}
