import { invoke } from "@tauri-apps/api/core";

export type VoiceEngine = "api" | "local";
export interface VoiceSettings {
  engine: VoiceEngine;
  api_url: string;
  api_model: string;
  local_bin: string;
  local_model: string;
  language: string;
  /** Para de gravar sozinho quando a fala termina. */
  auto_stop: boolean;
  /** Frase que, no fim do ditado, manda com Enter; vazia desliga. */
  send_phrase: string;
}
/** Quando ler a resposta do Claude sozinho: nunca (só pelo ▶), depois de um ditado, ou sempre. */
export type SpeakMode = "off" | "dictated" | "always";
export interface SpeechSettings {
  mode: SpeakMode;
  /** API no formato da OpenAI ou extensão de voz local (tipo tts, como a do Kokoro). */
  engine: "api" | "extension";
  /** Id da extensão de voz, quando engine = "extension". */
  extension: string;
  /** Usa a URL e a chave do ditado; o modelo e a voz são sempre os daqui. */
  same_api: boolean;
  api_url: string;
  api_model: string;
  voice: string;
}
/** As chaves das APIs nunca vêm pro frontend: só se existe uma salva. */
export interface SettingsView {
  voice: VoiceSettings;
  has_api_key: boolean;
  speech: SpeechSettings;
  has_speech_key: boolean;
  /** API local das extensões (extensions/). */
  api_enabled: boolean;
  api_error: string | null;
}

export const loadSettings = () => invoke<SettingsView>("settings_load");
export const setApiEnabled = (enabled: boolean) => invoke<SettingsView>("settings_set_api", { enabled });
/** Chaves: undefined mantém a salva, "" apaga. */
export const saveSettings = (voice: VoiceSettings, speech: SpeechSettings, apiKey?: string, speechKey?: string) =>
  invoke<SettingsView>("settings_save", { voice, apiKey: apiKey ?? null, speech, speechKey: speechKey ?? null });


/** Serviços conhecidos; qualquer API compatível com a da OpenAI funciona. */
export const VOICE_PRESETS = [
  { id: "groq", label: "Groq", url: "https://api.groq.com/openai/v1", model: "whisper-large-v3-turbo" },
  { id: "openai", label: "OpenAI", url: "https://api.openai.com/v1", model: "whisper-1" },
] as const;

export const voiceStart = () => invoke<void>("voice_start");
/** Para de gravar e devolve o texto transcrito. */
export const voiceStop = () => invoke<string>("voice_stop");
export const voiceCancel = () => invoke<void>("voice_cancel");

/** Saída de voz: só a do Groq e a da OpenAI têm preset; a OpenAI fala português. */
export const SPEECH_PRESETS = [
  { id: "groq", label: "Groq (só inglês)", url: "https://api.groq.com/openai/v1", model: "canopylabs/orpheus-v1-english", voice: "troy" },
  { id: "openai", label: "OpenAI", url: "https://api.openai.com/v1", model: "gpt-4o-mini-tts", voice: "alloy" },
] as const;

/** Voltam quando a fala acaba ou é interrompida. */
export const speakText = (text: string) => invoke<void>("speak_text", { text });
/** false quando não havia resposta pra ler. */
export const speakClaudeReply = (id: number) => invoke<boolean>("speak_claude_reply", { id });
export const speakStop = () => invoke<void>("speak_stop");
/** Extensões de voz instaladas: [id, nome]. */
export const installedTts = () => invoke<[string, string][]>("extensions_tts");

export const pickFiles = () => invoke<string[]>("pick_files");

/** Caminho pronto pro shell: aspas simples só quando precisa. */
export function shellQuote(path: string) {
  return /^[\w@%+=:,./~-]+$/.test(path) ? path : `'${path.replace(/'/g, `'\\''`)}'`;
}
