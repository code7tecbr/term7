<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref } from "vue";
import type { ClaudeUsage } from "../lib/pty";
import { loadSettings, saveSettings, setChatMode, VOICE_PRESETS, type VoiceSettings } from "../lib/settings";

/** Configurações do term7: modo chat do Claude e ditado por voz. */
defineProps<{ usage: ClaudeUsage | null }>();
const emit = defineEmits<{ close: []; chat: [enabled: boolean]; usage: [enabled: boolean] }>();

/** Liga na hora, sem o "Salvar": é só um interruptor. */
const chat = ref(false);
const chatStatus = ref("");
async function toggleChat() {
  const next = !chat.value;
  try {
    chat.value = (await setChatMode(next)).claude_chat;
    chatStatus.value = "";
    emit("chat", chat.value);
  } catch (err) {
    chatStatus.value = `Não deu pra salvar: ${err}`;
  }
}

const voice = reactive<VoiceSettings>({ engine: "api", api_url: "", api_model: "", local_bin: "", local_model: "", language: "" });
const hasKey = ref(false);
/** Chave digitada agora; vazio = mantém a salva. */
const newKey = ref("");
const status = ref<{ ok: boolean; text: string } | null>(null);
const saving = ref(false);

const preset = computed(() => VOICE_PRESETS.find((p) => p.url === voice.api_url.replace(/\/$/, ""))?.id ?? "custom");

function applyPreset(id: string) {
  const p = VOICE_PRESETS.find((x) => x.id === id);
  if (!p) return;
  voice.api_url = p.url;
  voice.api_model = p.model;
}

async function save(apiKey?: string) {
  saving.value = true;
  try {
    const view = await saveSettings({ ...voice }, apiKey);
    hasKey.value = view.has_api_key;
    newKey.value = "";
    status.value = { ok: true, text: "Salvo." };
  } catch (err) {
    status.value = { ok: false, text: `Não deu pra salvar: ${err}` };
  } finally {
    saving.value = false;
  }
}

function onKey(e: KeyboardEvent) {
  if (e.key !== "Escape") return;
  e.preventDefault();
  e.stopPropagation();
  emit("close");
}

onMounted(async () => {
  window.addEventListener("keydown", onKey, true);
  const view = await loadSettings().catch(() => null);
  if (view) {
    Object.assign(voice, view.voice);
    hasKey.value = view.has_api_key;
    chat.value = view.claude_chat;
  }
});
onBeforeUnmount(() => window.removeEventListener("keydown", onKey, true));
</script>

<template>
  <div class="backdrop" @mousedown.self="emit('close')">
    <div class="panel" role="dialog" aria-label="Configurações">
      <header>
        <span class="title">Configurações</span>
        <button class="close" title="Fechar (Esc)" @click="emit('close')">
          <svg viewBox="0 0 16 16" width="13" height="13"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" /></svg>
        </button>
      </header>

      <form class="body" @submit.prevent="save(newKey || undefined)">
        <section>
          <h2>Claude</h2>
          <label class="switch">
            <input type="checkbox" :checked="chat" @change="toggleChat" />
            <span class="track"><span class="knob" /></span>
            <span class="switch-text">
              Modo chat <span class="badge">experimental</span>
              <span class="hint">
                Mostra o botão <b>Chat</b> na barra de cima quando o Claude está rodando na aba. Ele troca o terminal por uma tela de
                conversa, continuando a mesma sessão; o botão <b>Terminal</b> volta. Se algo der errado no chat, volte pro terminal e
                compare.
              </span>
            </span>
          </label>
          <p v-if="chatStatus" class="status error">{{ chatStatus }}</p>
          <div class="usage-row">
            <span class="hint">
              <b>Uso na barra de cima</b> (5h, 7d e o do Fable nas contas Max):
              <template v-if="!usage">abra o Claude na aba pra ver ou mudar.</template>
              <template v-else-if="usage.installed">ativado em <code>{{ usage.settings_path }}</code>.</template>
              <template v-else>desativado. Ative pelo botão “Uso” na barra de cima.</template>
            </span>
            <button v-if="usage?.installed" type="button" class="secondary" @click="emit('usage', false)">Desativar</button>
          </div>
        </section>

        <section>
          <h2>Ditado por voz</h2>
          <p class="hint">
            O microfone da barra de cima grava, transcreve e cola o texto no terminal ativo, inclusive no Claude (sem Enter).
          </p>

          <div class="engines">
            <label :class="{ on: voice.engine === 'api' }">
              <input v-model="voice.engine" type="radio" value="api" />
              <span><b>Serviço externo</b><small>API compatível com a da OpenAI (Groq, OpenAI…). Rápido; o áudio sai da máquina.</small></span>
            </label>
            <label :class="{ on: voice.engine === 'local' }">
              <input v-model="voice.engine" type="radio" value="local" />
              <span><b>Local (whisper.cpp)</b><small>Nada sai da máquina. Precisa do binário e de um modelo baixado.</small></span>
            </label>
          </div>

          <template v-if="voice.engine === 'api'">
            <label class="field">
              <span>Serviço</span>
              <select :value="preset" @change="applyPreset(($event.target as HTMLSelectElement).value)">
                <option v-for="p in VOICE_PRESETS" :key="p.id" :value="p.id">{{ p.label }}</option>
                <option value="custom" disabled>Outro (URL abaixo)</option>
              </select>
            </label>
            <label class="field">
              <span>URL da API</span>
              <input v-model.trim="voice.api_url" placeholder="https://api.groq.com/openai/v1" spellcheck="false" />
            </label>
            <label class="field">
              <span>Modelo</span>
              <input v-model.trim="voice.api_model" placeholder="whisper-large-v3-turbo" spellcheck="false" />
            </label>
            <label class="field">
              <span>Chave da API</span>
              <div class="key">
                <input
                  v-model="newKey"
                  type="password"
                  autocomplete="off"
                  spellcheck="false"
                  :placeholder="hasKey ? 'Salva. Digite outra pra trocar' : 'gsk_… ou sk-…'"
                />
                <button v-if="hasKey" type="button" class="secondary" title="Apaga a chave salva" @click="save('')">Apagar</button>
              </div>
              <small>Fica em settings.json, legível só pelo seu usuário. Não aparece mais aqui depois de salva.</small>
            </label>
          </template>

          <template v-else>
            <label class="field">
              <span>Binário</span>
              <input v-model.trim="voice.local_bin" placeholder="whisper-cli" spellcheck="false" />
              <small>O <code>whisper-cli</code> do whisper.cpp: nome no PATH ou caminho completo.</small>
            </label>
            <label class="field">
              <span>Modelo</span>
              <input v-model.trim="voice.local_model" placeholder="~/modelos/ggml-small.bin" spellcheck="false" />
              <small>Arquivo .bin do whisper.cpp (o <code>small</code> vai bem em português).</small>
            </label>
          </template>

          <label class="field">
            <span>Idioma</span>
            <input v-model.trim="voice.language" placeholder="pt" spellcheck="false" class="short" />
            <small>Código ISO; vazio deixa o serviço detectar.</small>
          </label>
        </section>

        <footer>
          <span v-if="status" class="status" :class="{ error: !status.ok }">{{ status.text }}</span>
          <button type="submit" class="primary" :disabled="saving">Salvar</button>
        </footer>
      </form>
    </div>
  </div>
</template>

<style scoped>
.backdrop {
  position: fixed;
  inset: 0;
  z-index: 200;
  display: grid;
  place-items: center;
  background: rgb(0 0 0 / 0.45);
}
.panel {
  display: flex;
  flex-direction: column;
  width: min(560px, calc(100vw - 48px));
  max-height: calc(100vh - 48px);
  overflow: hidden;
  border: 1px solid var(--border);
  border-radius: 10px;
  background: var(--bg);
  box-shadow: 0 20px 48px rgb(0 0 0 / 0.5);
  font-size: 13px;
}
header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 8px 8px 16px;
  background: var(--bg-elevated);
  border-bottom: 1px solid var(--border);
}
.title {
  font-weight: 600;
}
.close {
  display: grid;
  place-items: center;
  width: 26px;
  height: 26px;
  border: 0;
  border-radius: 5px;
  background: none;
  color: var(--fg-muted);
  cursor: pointer;
}
.close:hover {
  background: var(--hover);
  color: var(--fg);
}
.body {
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow-y: auto;
}
section {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 14px 16px;
}
h2 {
  margin: 0;
  font-size: 13px;
  font-weight: 600;
}
.hint,
small {
  margin: 0;
  color: var(--fg-muted);
  font-size: 11.5px;
  line-height: 1.45;
}
code {
  font-family: var(--font-mono);
  font-size: 11.5px;
}
.engines {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px;
}
.engines label {
  display: flex;
  gap: 8px;
  padding: 10px;
  border: 1px solid var(--border);
  border-radius: 8px;
  cursor: pointer;
}
.engines label.on {
  border-color: var(--accent);
  background: color-mix(in srgb, var(--accent) 8%, transparent);
}
.engines input {
  margin: 2px 0 0;
  accent-color: var(--accent);
}
.engines span {
  display: flex;
  flex-direction: column;
  gap: 3px;
}
.field {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.field > span {
  color: var(--fg-muted);
  font-size: 12px;
}
input:not([type="radio"]),
select {
  height: 30px;
  padding: 0 10px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--bg-elevated);
  color: var(--fg);
  font: inherit;
  font-size: 12.5px;
}
input:focus,
select:focus {
  outline: none;
  border-color: var(--accent);
}
input.short {
  width: 90px;
}
.key {
  display: flex;
  gap: 6px;
}
.key input {
  flex: 1;
}
.usage-row {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-top: 12px;
}
.usage-row .hint {
  flex: 1;
}
.switch {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  cursor: pointer;
}
.switch input {
  position: absolute;
  opacity: 0;
  pointer-events: none;
}
.track {
  position: relative;
  flex-shrink: 0;
  width: 30px;
  height: 17px;
  margin-top: 1px;
  border-radius: 9px;
  background: var(--active);
  transition: background 0.15s;
}
.knob {
  position: absolute;
  top: 2px;
  left: 2px;
  width: 13px;
  height: 13px;
  border-radius: 50%;
  background: var(--fg-muted);
  transition: transform 0.15s, background 0.15s;
}
.switch input:checked + .track {
  background: color-mix(in srgb, var(--claude) 45%, transparent);
}
.switch input:checked + .track .knob {
  transform: translateX(13px);
  background: var(--claude);
}
.switch input:focus-visible + .track {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}
.switch-text {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.badge {
  margin-left: 4px;
  padding: 1px 6px;
  border-radius: 8px;
  background: color-mix(in srgb, var(--yellow) 18%, transparent);
  color: var(--yellow);
  font-size: 10.5px;
}
footer {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 10px;
  padding: 10px 16px;
  border-top: 1px solid var(--border);
}
.status {
  flex: 1;
  color: var(--green);
  font-size: 12px;
}
.status.error {
  color: var(--red);
}
button.primary,
button.secondary {
  height: 30px;
  padding: 0 14px;
  border-radius: 6px;
  font: inherit;
  font-size: 12.5px;
  cursor: pointer;
}
button.primary {
  border: 0;
  background: var(--accent);
  color: var(--bg);
  font-weight: 600;
}
button.primary:disabled {
  opacity: 0.6;
}
button.secondary {
  border: 1px solid var(--border);
  background: none;
  color: var(--fg);
}
button.secondary:hover {
  background: var(--hover);
}
</style>
