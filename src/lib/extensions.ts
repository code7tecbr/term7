import { invoke } from "@tauri-apps/api/core";

/** Campo de configuração declarado no extension.json. */
export interface ConfigField {
  key: string;
  label: string;
  type: "text" | "url" | "secret" | "number";
  required: boolean;
  default: string | null;
  placeholder: string | null;
  help: string | null;
}

export interface ClaudeAccount {
  id: string;
  label: string;
  installed: boolean;
}

export interface Extension {
  id: string;
  name: string;
  description: string;
  link: string | null;
  /** service: serviço do systemd; mcp: MCP nas contas do Claude; tts: voz local da leitura em voz alta. */
  kind: "service" | "mcp" | "tts";
  config: ConfigField[];
  /** Valores salvos, sem os secretos. */
  values: Record<string, string>;
  /** Secretos que já têm valor salvo (o valor nunca vem pra cá). */
  secrets_set: string[];
  installed: boolean;
  running: boolean;
  accounts: ClaudeAccount[];
}

export const listExtensions = () => invoke<Extension[]>("extensions_list");

/** `config`: secreto null mantém o salvo, "" apaga. `accounts`: contas do Claude (tipo mcp). */
export const installExtension = (id: string, config: Record<string, string | null>, accounts: string[]) =>
  invoke<Extension>("extension_install", { id, config, accounts });

export const uninstallExtension = (id: string) => invoke<Extension>("extension_uninstall", { id });

/** Valores iniciais do formulário: o salvo, senão o padrão; secretos começam vazios. */
export function formValues(ext: Extension): Record<string, string> {
  return Object.fromEntries(ext.config.map((f) => [f.key, f.type === "secret" ? "" : (ext.values[f.key] ?? f.default ?? "")]));
}

/** O que vai pro backend: secreto vazio e já salvo = mantém (null), a não ser que peçam pra apagar. */
export function configToSend(ext: Extension, form: Record<string, string | number>, clear: Set<string> = new Set()) {
  return Object.fromEntries(
    ext.config.map((f) => {
      // O v-model de <input type="number"> devolve número; o backend guarda tudo como texto.
      const v = String(form[f.key] ?? "");
      if (f.type === "secret" && !v) return [f.key, clear.has(f.key) ? "" : null];
      return [f.key, v];
    }),
  );
}
