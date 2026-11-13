<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref } from "vue";
import type { ClaudeUsage } from "../lib/pty";
import ExtensionsList from "./ExtensionsList.vue";
import {
  installedTts,
  loadSettings,
  saveSettings,
  setApiEnabled,
  speakText,
  SPEECH_PRESETS,
  VOICE_PRESETS,
  type SpeechSettings,
  type VoiceSettings,
} from "../lib/settings";

/** Configurações do term7: uso do Claude na barra, ditado e leitura em voz alta. */
defineProps<{ usage: ClaudeUsage | null }>();
const emit = defineEmits<{ close: []; usage: [enabled: boolean] }>();

const voice = reactive<VoiceSettings>({
  engine: "api",
  api_url: "",
  api_model: "",
  local_bin: "",
  local_model: "",
  language: "",
  auto_stop: true,
  send_phrase: "term7 pronto",
});
const hasKey = ref(false);
/** Chave digitada agora; vazio = mantém a salva. */
const newKey = ref("");
const speech = reactive<SpeechSettings>({
  mode: "off",
  engine: "api",
  extension: "kokoro",
  same_api: true,
  api_url: "",
  api_model: "",
  voice: "",
});
/** Extensões de voz instaladas: [id, nome]. */
const ttsExtensions = ref<[string, string][]>([]);

/** As três fontes da voz numa escolha só (no backend são engine + same_api). */
const speechSource = computed({
  get: () => (speech.engine === "extension" ? "extension" : speech.same_api ? "dictation" : "other"),
  set: (v: "dictation" | "other" | "extension") => {
    speech.engine = v === "extension" ? "extension" : "api";
    if (v !== "extension") speech.same_api = v === "dictation";
    else if (!ttsExtensions.value.some(([id]) => id === speech.extension) && ttsExtensions.value.length) {
      speech.extension = ttsExtensions.value[0][0];
    }
  },
});

async function refreshTts() {
  ttsExtensions.value = await installedTts().catch(() => []);
}
const hasSpeechKey = ref(false);
const newSpeechKey = ref("");
const testing = ref(false);
const status = ref<{ ok: boolean; text: string } | null>(null);
const saving = ref(false);
const api = ref<{ enabled: boolean; error: string | null }>({ enabled: false, error: null });

/** Instalar uma extensão liga a API: relê o estado pra o botão acompanhar. */
async function refreshApi() {
  const view = await loadSettings().catch(() => null);
  if (view) api.value = { enabled: view.api_enabled, error: view.api_error };
  // Instalar ou remover uma extensão de voz muda as opções da leitura em voz alta.
  await refreshTts();
}

async function toggleApi() {
  try {
    const view = await setApiEnabled(!api.value.enabled);
    api.value = { enabled: view.api_enabled, error: view.api_error };
  } catch (err) {
    api.value = { ...api.value, error: `Não deu pra salvar: ${err}` };
  }
}

const preset = computed(() => VOICE_PRESETS.find((p) => p.url === voice.api_url.replace(/\/$/, ""))?.id ?? "custom");

function applyPreset(id: string) {
  const p = VOICE_PRESETS.find((x) => x.id === id);
  if (!p) return;
  voice.api_url = p.url;
  voice.api_model = p.model;
}

// O preset da voz é reconhecido pelo modelo: com "mesma API" a URL é a do ditado.
const speechPreset = computed(() => SPEECH_PRESETS.find((p) => p.model === speech.api_model)?.id ?? "custom");

function applySpeechPreset(id: string) {
  const p = SPEECH_PRESETS.find((x) => x.id === id);
  if (!p) return;
  speech.api_url = p.url;
  speech.api_model = p.model;
  speech.voice = p.voice;
}

/** Chaves: undefined mantém a salva (ou usa a digitada), "" apaga. */
async function save(keys: { api?: string; speech?: string } = {}) {
  saving.value = true;
  try {
    const view = await saveSettings(
      { ...voice },
      { ...speech },
      keys.api ?? (newKey.value || undefined),
      keys.speech ?? (newSpeechKey.value || undefined),
    );
    hasKey.value = view.has_api_key;
    hasSpeechKey.value = view.has_speech_key;
    newKey.value = "";
    newSpeechKey.value = "";
    status.value = { ok: true, text: "Salvo." };
    return true;
  } catch (err) {
    status.value = { ok: false, text: `Não deu pra salvar: ${err}` };
    return false;
  } finally {
    saving.value = false;
  }
}

/** Salva antes: o backend fala com a configuração gravada. */
async function testSpeech() {
  if (!(await save())) return;
  testing.value = true;
  try {
    await speakText("Olá! Esta é a voz que vai ler as respostas do Claude.");
    status.value = { ok: true, text: "Teste de voz concluído." };
  } catch (err) {
    status.value = { ok: false, text: `Voz: ${err}` };
  } finally {
    testing.value = false;
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
    Object.assign(speech, view.speech);
    hasSpeechKey.value = view.has_speech_key;
    await refreshTts();
    api.value = { enabled: view.api_enabled, error: view.api_error };
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

      <form class="body" @submit.prevent="save()">
        <section>
          <h2>Claude</h2>
          <div class="usage-row">
            <span class="hint">
              <b>Uso na barra de cima</b> (contexto, 5h, 7d e os limites por modelo, como o do Fable):
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
            O microfone da barra de cima (ou <b>Ctrl+Espaço</b>) grava, transcreve e cola o texto no terminal ativo, inclusive
            no Claude (sem Enter). Termine a fala com a frase de enviar (abaixo) pra mandar com Enter.
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
                <button v-if="hasKey" type="button" class="secondary" title="Apaga a chave salva" @click="save({ api: '' })">Apagar</button>
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

          <label class="field">
            <span>Frase de enviar</span>
            <input v-model.trim="voice.send_phrase" placeholder="term7 pronto" spellcheck="false" />
            <small>
              Dita no fim, manda com Enter. “term7” aceita como o Whisper escreve (Term 7, Terme 7, Term set, term sete);
              acento e pontuação não importam. Vazia desliga.
            </small>
          </label>

          <label class="check">
            <input v-model="voice.auto_stop" type="checkbox" />
            <span>
              <b>Parar sozinho quando eu parar de falar</b>
              <small>Transcreve depois de 1,5 s de silêncio; sem fala em 8 s, descarta. Desligado, clique de novo pra parar.</small>
            </span>
          </label>
        </section>

        <section>
          <h2>Leitura em voz alta</h2>
          <p class="hint">
            Lê a resposta do Claude quando ele termina, sem blocos de código, tabelas e links. Pela API, respostas longas são
            lidas só no começo (por causa da cota); a voz local lê inteira. O <b>▶</b> ao lado do microfone (ou
            <b>Ctrl+Shift+Espaço</b>) lê a última resposta da aba na hora e, falando, vira o botão de parar.
          </p>

          <label class="field">
            <span>Ler a resposta</span>
            <select v-model="speech.mode">
              <option value="off">Só quando eu pedir (▶ ou Ctrl+Shift+Espaço)</option>
              <option value="dictated">Quando a pergunta foi ditada</option>
              <option value="always">Sempre que o Claude da aba ativa terminar</option>
            </select>
          </label>

          <div class="engines three">
            <label :class="{ on: speechSource === 'dictation' }">
              <input v-model="speechSource" type="radio" value="dictation" />
              <span><b>Mesma API do ditado</b><small>Usa a URL e a chave de cima; só o modelo e a voz são daqui.</small></span>
            </label>
            <label :class="{ on: speechSource === 'other' }">
              <input v-model="speechSource" type="radio" value="other" />
              <span><b>Outra API</b><small>URL e chave próprias (ex.: ditado no Groq e voz na OpenAI).</small></span>
            </label>
            <label :class="{ on: speechSource === 'extension', disabled: !ttsExtensions.length }">
              <input v-model="speechSource" type="radio" value="extension" :disabled="!ttsExtensions.length" />
              <span>
                <b>Voz local (extensão)</b>
                <small v-if="ttsExtensions.length">Gerada na máquina, sem internet nem cota; usa CPU a cada resposta.</small>
                <small v-else>Instale “Voz local (Kokoro)” em Extensões, aqui embaixo.</small>
              </span>
            </label>
          </div>

          <label v-if="speech.engine === 'extension' && ttsExtensions.length > 1" class="field">
            <span>Extensão</span>
            <select v-model="speech.extension">
              <option v-for="[id, name] in ttsExtensions" :key="id" :value="id">{{ name }}</option>
            </select>
          </label>
          <p v-if="speech.engine === 'extension'" class="hint">
            Voz, idioma, velocidade e núcleos da CPU ficam na configuração da extensão, em Extensões.
          </p>

          <label v-if="speech.engine === 'api'" class="field">
            <span>Serviço</span>
            <select :value="speechPreset" @change="applySpeechPreset(($event.target as HTMLSelectElement).value)">
              <option v-for="p in SPEECH_PRESETS" :key="p.id" :value="p.id">{{ p.label }}</option>
              <option value="custom" disabled>Outro (modelo abaixo)</option>
            </select>
            <small v-if="speech.same_api">O serviço precisa ser o mesmo do ditado: a URL e a chave vêm de lá.</small>
            <small v-else-if="speechPreset === 'groq'">Os modelos de voz do Groq falam só inglês (e árabe). Pra português, use a OpenAI.</small>
          </label>

          <template v-if="speech.engine === 'api' && !speech.same_api">
            <label class="field">
              <span>URL da API</span>
              <input v-model.trim="speech.api_url" placeholder="https://api.openai.com/v1" spellcheck="false" />
            </label>
            <label class="field">
              <span>Chave da API</span>
              <div class="key">
                <input
                  v-model="newSpeechKey"
                  type="password"
                  autocomplete="off"
                  spellcheck="false"
                  :placeholder="hasSpeechKey ? 'Salva. Digite outra pra trocar' : 'sk-… ou gsk_…'"
                />
                <button v-if="hasSpeechKey" type="button" class="secondary" title="Apaga a chave salva" @click="save({ speech: '' })">
                  Apagar
                </button>
              </div>
            </label>
          </template>

          <template v-if="speech.engine === 'api'">
            <label class="field">
              <span>Modelo</span>
              <input v-model.trim="speech.api_model" placeholder="gpt-4o-mini-tts" spellcheck="false" />
            </label>
            <label class="field">
              <span>Voz</span>
              <input v-model.trim="speech.voice" placeholder="alloy" spellcheck="false" />
              <small>Nome da voz do serviço (Groq: troy, hannah, austin…; OpenAI: alloy, nova, shimmer…).</small>
            </label>
          </template>
          <div class="test">
            <button type="button" class="secondary" :disabled="testing || saving" @click="testSpeech">
              {{ testing ? "Falando…" : "Testar voz" }}
            </button>
          </div>
        </section>

        <section>
          <h2>Extensões</h2>
          <div class="usage-row">
            <span class="hint">
              <b>API local</b>: deixa as extensões (como o MCP do term7, em <code>extensions/</code>) listar as abas, ler a tela
              e mandar mensagem pro Claude de uma aba. Só o seu usuário acessa (socket em
              <code>$XDG_RUNTIME_DIR/term7/api.sock</code>).
            </span>
            <button type="button" :class="api.enabled ? 'secondary' : 'primary'" @click="toggleApi">
              {{ api.enabled ? "Desligar" : "Ligar" }}
            </button>
          </div>
          <small v-if="api.error" class="api-error">{{ api.error }}</small>
          <ExtensionsList @changed="refreshApi" />
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
.api-error {
  color: var(--red);
}
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
.check {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  cursor: pointer;
}
.check input {
  margin-top: 3px;
}
.check span {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.engines.three {
  grid-template-columns: repeat(3, 1fr);
}
.engines label.disabled {
  opacity: 0.55;
  cursor: default;
}
.test {
  display: flex;
  justify-content: flex-end;
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
