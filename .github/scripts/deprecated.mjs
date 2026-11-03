// Lista os pacotes do package-lock.json que o npm marcou como descontinuados
// (o aviso "deprecated" do npm install). Imprime Markdown pra issue; sem nenhum, não imprime nada.
import { readFileSync } from "node:fs";

const lock = JSON.parse(readFileSync(process.argv[2] ?? "package-lock.json", "utf8"));
const root = lock.packages[""] ?? {};
const direct = new Set([...Object.keys(root.dependencies ?? {}), ...Object.keys(root.devDependencies ?? {})]);

// "node_modules/a/node_modules/@b/c" -> "@b/c"; a mesma versão pode aparecer em vários lugares.
const installed = new Map();
for (const [path, info] of Object.entries(lock.packages)) {
  if (!path || info.link || !info.version) continue;
  const name = info.name ?? path.slice(path.lastIndexOf("node_modules/") + "node_modules/".length);
  installed.set(`${name}@${info.version}`, { name, version: info.version, dev: !!info.dev });
}

async function deprecation({ name, version }) {
  for (let attempt = 0; attempt < 3; attempt++) {
    const res = await fetch(`https://registry.npmjs.org/${name.replace("/", "%2F")}/${version}`).catch(() => null);
    if (res?.ok) return (await res.json()).deprecated ?? null;
    if (res?.status === 404) return null;
    await new Promise((r) => setTimeout(r, 1000 * (attempt + 1)));
  }
  throw new Error(`não consegui consultar ${name}@${version} no npm`);
}

const list = [...installed.values()];
const found = [];
// Poucas consultas por vez, pra não levar bloqueio do registry.
for (let i = 0; i < list.length; i += 16) {
  const batch = list.slice(i, i + 16);
  const results = await Promise.all(batch.map(deprecation));
  batch.forEach((pkg, j) => results[j] && found.push({ ...pkg, message: results[j] }));
}

if (found.length) {
  const cell = (s) => String(s).replace(/\|/g, "\\|").replace(/\s+/g, " ").trim();
  const rows = (pkgs) => pkgs.map((p) => `| \`${p.name}@${p.version}\` | ${p.dev ? "só no build" : "app"} | ${cell(p.message)} |`);
  const header = ["| Pacote | Onde | Aviso do npm |", "| --- | --- | --- |"];
  const own = found.filter((p) => direct.has(p.name));
  const deps = found.filter((p) => !direct.has(p.name));
  const out = ["## npm", "", "Pacotes do `package-lock.json` marcados como descontinuados no npm."];
  if (own.length) out.push("", "**Usados direto pelo projeto**", "", ...header, ...rows(own));
  if (deps.length) out.push("", "**Dependências de outros pacotes** (saem quando o pacote que as puxa for atualizado)", "", ...header, ...rows(deps));
  console.log(out.join("\n"));
}
