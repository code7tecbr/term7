import { invoke } from "@tauri-apps/api/core";

export type VoiceEngine = "api" | "local";
export interface VoiceSettings {
  engine: VoiceEngine;
  api_url: string;
  api_model: string;
  local_bin: string;
  local_model: string;
  language: string;
}
/** A chave da API nunca vem pro frontend: só se existe uma salva. */
export interface SettingsView {
  voice: VoiceSettings;
  has_api_key: boolean;
}

export const loadSettings = () => invoke<SettingsView>("settings_load");
/** apiKey: undefined mantém a salva, "" apaga. */
export const saveSettings = (voice: VoiceSettings, apiKey?: string) =>
  invoke<SettingsView>("settings_save", { voice, apiKey: apiKey ?? null });

/** Serviços conhecidos; qualquer API compatível com a da OpenAI funciona. */
export const VOICE_PRESETS = [
  { id: "groq", label: "Groq", url: "https://api.groq.com/openai/v1", model: "whisper-large-v3-turbo" },
  { id: "openai", label: "OpenAI", url: "https://api.openai.com/v1", model: "whisper-1" },
] as const;

export const voiceStart = () => invoke<void>("voice_start");
/** Para de gravar e devolve o texto transcrito. */
export const voiceStop = () => invoke<string>("voice_stop");
export const voiceCancel = () => invoke<void>("voice_cancel");

export const pickFiles = () => invoke<string[]>("pick_files");

/** Caminho pronto pro shell: aspas simples só quando precisa. */
export function shellQuote(path: string) {
  return /^[\w@%+=:,./~-]+$/.test(path) ? path : `'${path.replace(/'/g, `'\\''`)}'`;
}
