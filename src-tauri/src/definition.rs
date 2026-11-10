//! "Ir pra definição" (Ctrl+clique no visualizador): sem servidor de linguagem.
//! Procura o nome com `git grep` e fica com as linhas que têm cara de definição
//! (`function x`, `class X`, `fn x`, `def x`, `const x =`...). Erra às vezes,
//! mas funciona em qualquer linguagem e não depende de nada instalado.

use std::{path::Path, process::Command};

use serde::Serialize;

/// Nomes muito comuns geram saída enorme; isso basta pra achar a definição.
const MAX_GREP_LINES: usize = 5_000;
const MAX_RESULTS: usize = 30;
/// Linha maior que isso é código minificado: não é onde alguém define algo.
const MAX_LINE_LEN: usize = 400;

#[derive(Serialize, Debug, PartialEq)]
pub struct Definition {
    /// Relativo à pasta pesquisada (a raiz do arquivo aberto).
    path: String,
    line: u32,
    text: String,
}

/// Palavra que, logo antes do nome, indica que a linha o define.
const DEF_KEYWORDS: &[&str] = &[
    "function", "function*", "func", "fn", "def", "class", "interface", "type", "enum", "struct", "trait",
    "union", "mod", "module", "namespace", "macro_rules!", "record", "object", "static",
];
/// Variáveis: definem, mas podem ser só locais.
const VAR_KEYWORDS: &[&str] = &["const", "let", "var", "val"];
/// Podem vir antes de um método (`async foo() {`, `public static foo() {`).
const MODIFIERS: &[&str] = &[
    "export", "default", "declare", "public", "private", "protected", "static", "async", "get", "set", "readonly",
    "override", "abstract", "pub", "pub(crate)", "unsafe", "final", "virtual", "inline",
];
/// Antes de `nome(...) {` indicam chamada ou controle, não definição.
const NOT_TYPES: &[&str] = &[
    "return", "new", "await", "else", "throw", "yield", "case", "typeof", "in", "of", "if", "while", "for",
    "switch", "catch", "do", "delete", "echo", "not", "and", "or",
];

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

pub fn valid_symbol(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_alphabetic() || c == '_' || c == '$') && chars.all(is_ident) && s.len() <= 200
}

/// Quão provável é a linha definir `symbol` (0 = certeza, 3 = palpite), ou None.
fn rank(line: &str, symbol: &str) -> Option<u8> {
    let trimmed = line.trim();
    if trimmed.len() > MAX_LINE_LEN || ["//", "/*", "*", "#", "--", "<!--"].iter().any(|c| trimmed.starts_with(c)) {
        // `#` cobre comentário de Python/shell; `#[derive]` também não define nada.
        return None;
    }
    let top_level = !line.starts_with([' ', '\t']);
    let mut best: Option<u8> = None;
    for (i, _) in line.match_indices(symbol) {
        let before_ok = !line[..i].chars().next_back().is_some_and(is_ident);
        let after_ok = !line[i + symbol.len()..].chars().next().is_some_and(is_ident);
        if !(before_ok && after_ok) {
            continue;
        }
        if let Some(r) = rank_at(line[..i].trim(), line[i + symbol.len()..].trim_start(), trimmed, top_level) {
            best = Some(best.map_or(r, |b| b.min(r)));
        }
    }
    best
}

fn rank_at(before: &str, after: &str, trimmed: &str, top_level: bool) -> Option<u8> {
    let last = before.split_whitespace().next_back().unwrap_or("");
    if DEF_KEYWORDS.contains(&last) {
        return Some(0);
    }
    if VAR_KEYWORDS.contains(&last) && (after.starts_with('=') || after.starts_with(':')) {
        return Some(if top_level || before.contains("export") { 0 } else { 1 });
    }
    // Método do Go: `func (r *Repo) Nome(`.
    if before.starts_with("func (") && before.ends_with(')') {
        return Some(0);
    }
    let opens_block = trimmed.ends_with('{') && !trimmed.ends_with(";{");
    let call_like = after.starts_with('(') || after.starts_with('<');
    // `foo(() => {` é chamada com callback, não definição.
    if !opens_block || !call_like || trimmed.contains("=>") {
        return field_arrow(before, after);
    }
    // Método de classe/objeto: `async foo(a) {`, `foo() {`.
    if call_like && before.split_whitespace().all(|w| MODIFIERS.contains(&w)) {
        return Some(2);
    }
    // C, Java, C#...: `static int foo(int a) {` (o tipo vem antes do nome).
    let typed = last.chars().next_back().is_some_and(|c| is_ident(c) || "*&>]".contains(c));
    let first = trimmed.split(|c: char| !is_ident(c)).next().unwrap_or("");
    if call_like && typed && !NOT_TYPES.contains(&last) && !NOT_TYPES.contains(&first) && !before.contains(['=', '.', '(', ',']) {
        return Some(3);
    }
    None
}

/// Campo de classe com arrow function: `handle = (e) => {`.
fn field_arrow(before: &str, after: &str) -> Option<u8> {
    let modifiers_only = before.split_whitespace().all(|w| MODIFIERS.contains(&w));
    (modifiers_only && after.starts_with('=') && !after.starts_with("==") && after.contains("=>")).then_some(2)
}

/// Lê a saída de `git grep -n -z` (`caminho\0linha\0texto`) e ordena as definições.
fn parse(out: &str, symbol: &str, current: &str) -> Vec<Definition> {
    let mut found: Vec<(u8, bool, Definition)> = out
        .lines()
        .take(MAX_GREP_LINES)
        .filter_map(|l| {
            let mut parts = l.splitn(3, '\0');
            let (path, line, text) = (parts.next()?, parts.next()?.parse().ok()?, parts.next()?);
            let r = rank(text, symbol)?;
            Some((r, path == current, Definition { path: path.to_string(), line, text: text.trim().to_string() }))
        })
        .collect();
    // No próprio arquivo a definição ganha um nível (variável local, função privada...).
    let score = |(r, same, _): &(u8, bool, Definition)| (*r as i32 - *same as i32, !same);
    found.sort_by(|a, b| score(a).cmp(&score(b)).then_with(|| a.2.path.cmp(&b.2.path)).then(a.2.line.cmp(&b.2.line)));
    // Havendo definição clara (palavra-chave), os palpites só atrapalham.
    if found.iter().any(|(r, ..)| *r <= 1) {
        found.retain(|(r, ..)| *r <= 1);
    }
    found.into_iter().take(MAX_RESULTS).map(|(.., d)| d).collect()
}

fn find(root: &str, file: &str, symbol: &str) -> Vec<Definition> {
    if !valid_symbol(symbol) {
        return Vec::new();
    }
    let in_repo = Command::new("git")
        .args(["-C", root, "rev-parse", "--is-inside-work-tree"])
        .output()
        .is_ok_and(|o| o.status.success());
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(root).args(["grep", "-n", "-z", "-I", "-w", "-F", "--no-color"]);
    if in_repo {
        // Arquivos novos também, mas sem os ignorados (node_modules, target...).
        cmd.args(["--untracked", "-e", symbol]);
    } else {
        // Fora de um repositório só dá pra olhar o próprio arquivo.
        cmd.args(["--no-index", "-e", symbol, "--", file]);
    }
    let Ok(out) = cmd.env("GIT_OPTIONAL_LOCKS", "0").output() else { return Vec::new() };
    parse(&String::from_utf8_lossy(&out.stdout), symbol, file)
}

#[tauri::command]
pub async fn find_definitions(root: String, file: String, symbol: String) -> Result<Vec<Definition>, String> {
    tauri::async_runtime::spawn_blocking(move || find(&root, &file, &symbol))
        .await
        .map_err(|e| e.to_string())
}

// ---- imports: `from "./foo"` abre o arquivo ----

const IMPORT_EXTS: &[&str] = &["ts", "tsx", "js", "jsx", "mjs", "cjs", "vue", "d.ts", "rs", "py", "go", "css", "scss"];

/// Caminho de um import relativo (`./x`, `../y/z`) a partir da pasta do arquivo,
/// tentando as extensões comuns e o `index` de pasta, como fazem os bundlers.
fn resolve_import(dir: &Path, spec: &str) -> Option<String> {
    if !(spec.starts_with("./") || spec.starts_with("../")) {
        return None;
    }
    let base = dir.join(spec);
    let mut candidates = vec![base.clone()];
    let name = base.file_name()?.to_string_lossy().into_owned();
    candidates.extend(IMPORT_EXTS.iter().map(|e| base.with_file_name(format!("{name}.{e}"))));
    candidates.extend(IMPORT_EXTS.iter().map(|e| base.join(format!("index.{e}"))));
    candidates.push(base.join("mod.rs"));
    candidates.push(base.join("__init__.py"));
    candidates
        .into_iter()
        .find(|p| p.is_file())
        .and_then(|p| p.canonicalize().ok())
        .map(|p| p.to_string_lossy().into_owned())
}

/// Caminho absoluto do arquivo importado, ou None.
#[tauri::command]
pub async fn resolve_import_path(dir: String, spec: String) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || resolve_import(Path::new(&dir), &spec))
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconhece_definicoes_por_palavra_chave() {
        for line in [
            "export function openFile(file: OpenFile) {",
            "export default async function openFile() {",
            "pub fn openFile(root: &str) -> bool {",
            "    pub(crate) async fn openFile() {",
            "def openFile(self, path):",
            "class openFile:",
            "export interface openFile {",
            "export type openFile = { root: string };",
            "pub struct openFile {",
            "enum openFile {",
            "func openFile(path string) error {",
            "func (r *Repo) openFile(path string) error {",
            "export const openFile = (file) => {",
            "const openFile = computed(() => 1);",
        ] {
            assert_eq!(rank(line, "openFile"), Some(0), "{line}");
        }
    }

    #[test]
    fn variavel_local_vale_menos() {
        assert_eq!(rank("  const openFile = 1;", "openFile"), Some(1));
        assert_eq!(rank("    let openFile: bool = true;", "openFile"), Some(1));
    }

    #[test]
    fn metodos_e_funcoes_tipadas_sao_palpites() {
        assert_eq!(rank("  async openFile(path: string) {", "openFile"), Some(2));
        assert_eq!(rank("  openFile() {", "openFile"), Some(2));
        assert_eq!(rank("  handle = (e: Event) => {", "handle"), Some(2));
        assert_eq!(rank("public static int openFile(String p) {", "openFile"), Some(3));
    }

    #[test]
    fn ignora_usos_e_comentarios() {
        for line in [
            "  openFile(file, tab);",
            "import { openFile } from \"./editor\";",
            "  const x = openFile(a);",
            "  openFile(() => {",
            "  if (openFile(x)) {",
            "  return openFile(x) {",
            "  this.openFile(a) {",
            "// function openFile() {",
            " * class openFile",
            "# def openFile():",
            "  const reopenFile = 1;",
            "let x: openFile = y;",
        ] {
            assert_eq!(rank(line, "openFile"), None, "{line}");
        }
    }

    #[test]
    fn simbolo_valido() {
        assert!(valid_symbol("openFile"));
        assert!(valid_symbol("_x1"));
        assert!(valid_symbol("$store"));
        assert!(!valid_symbol("1abc"));
        assert!(!valid_symbol("-e"));
        assert!(!valid_symbol("a b"));
        assert!(!valid_symbol(""));
    }

    #[test]
    fn ordena_e_descarta_palpites_quando_ha_definicao() {
        let out = [
            "src/a.ts\x0010\x00  openFile(x);",
            "src/b.java\x003\x00public void openFile(int a) {",
            "src/state/editor.ts\x005\x00export function openFile(file: OpenFile) {",
            "src/App.vue\x0040\x00  const openFile = 2;",
        ]
        .join("\n");
        let defs = parse(&out, "openFile", "src/App.vue");
        let got: Vec<_> = defs.iter().map(|d| (d.path.as_str(), d.line)).collect();
        // A local do próprio arquivo sobe um nível e empata com a exportada; ganha no desempate.
        assert_eq!(got, [("src/App.vue", 40), ("src/state/editor.ts", 5)]);
    }

    #[test]
    fn sem_definicao_clara_mostra_palpites() {
        let out = "src/b.java\x003\x00public void openFile(int a) {\nsrc/a.ts\x0010\x00  openFile(x);";
        assert_eq!(parse(out, "openFile", "src/a.ts").len(), 1);
    }

    #[test]
    fn resolve_import_relativo() {
        let dir = std::env::temp_dir().join(format!("term7-def-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("lib/util")).unwrap();
        std::fs::write(dir.join("lib/git.ts"), "").unwrap();
        std::fs::write(dir.join("lib/util/index.ts"), "").unwrap();
        std::fs::write(dir.join("View.vue"), "").unwrap();
        let src = dir.join("components");
        std::fs::create_dir_all(&src).unwrap();
        let ok = |spec: &str, want: &str| {
            let got = resolve_import(&src, spec).unwrap();
            assert!(got.ends_with(want), "{spec} -> {got}");
        };
        ok("../lib/git", "lib/git.ts");
        ok("../lib/util", "lib/util/index.ts");
        ok("../View.vue", "View.vue");
        assert_eq!(resolve_import(&src, "../lib/nada"), None);
        assert_eq!(resolve_import(&src, "vue"), None);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
