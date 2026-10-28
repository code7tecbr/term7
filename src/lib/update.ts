import { invoke } from "@tauri-apps/api/core";
import { saveSession } from "../state/session";

export interface UpdateInfo {
  /** Versão da última release, sem o "v". */
  latest: string;
  newer: boolean;
  /** Rodando a versão do apt (só ela pode ser atualizada pelo botão). */
  installed: boolean;
}

export const checkUpdate = () => invoke<UpdateInfo>("update_check");
/** Baixa e instala o .deb; o sistema pede a senha. */
export const installUpdate = (version: string) => invoke<void>("update_install", { version });

/** Reinicia na versão nova; salva o layout antes pra voltar com as mesmas abas. */
export async function restartApp() {
  await saveSession().catch(() => {});
  await invoke("update_restart");
}
