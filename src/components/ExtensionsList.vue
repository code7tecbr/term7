<script setup lang="ts">
import { onMounted, reactive, ref } from "vue";
import { openUrl } from "../lib/github";
import { configToSend, formValues, installExtension, listExtensions, uninstallExtension, type Extension } from "../lib/extensions";

/** Extensões que vêm com o term7: instalar, configurar e remover. */
const emit = defineEmits<{ changed: [] }>();

const exts = ref<Extension[] | null>(null);
const loadError = ref<string | null>(null);
const open = ref<string | null>(null);
const forms = reactive<Record<string, Record<string, string>>>({});
const accounts = reactive<Record<string, string[]>>({});
/** Secretos que o usuário pediu pra apagar. */
const clearing = reactive<Record<string, Set<string>>>({});
const busy = ref<string | null>(null);
const status = reactive<Record<string, { ok: boolean; text: string }>>({});

function reset(ext: Extension) {
  forms[ext.id] = formValues(ext);
  accounts[ext.id] = ext.accounts.filter((a) => a.installed).map((a) => a.id);
  // MCP ainda não instalado: já sugere todas as contas.
  if (ext.kind === "mcp" && !ext.installed) accounts[ext.id] = ext.accounts.map((a) => a.id);
  clearing[ext.id] = new Set();
}

function replace(ext: Extension) {
  const i = exts.value?.findIndex((e) => e.id === ext.id) ?? -1;
  if (exts.value && i >= 0) exts.value[i] = ext;
  reset(ext);
}

async function load() {
  try {
    exts.value = await listExtensions();
    exts.value.forEach(reset);
  } catch (err) {
    loadError.value = String(err);
  }
}

async function run(ext: Extension, action: () => Promise<Extension>, ok: string) {
  busy.value = ext.id;
  try {
    replace(await action());
    status[ext.id] = { ok: true, text: ok };
    emit("changed");
  } catch (err) {
    status[ext.id] = { ok: false, text: String(err) };
  } finally {
    busy.value = null;
  }
}

const install = (ext: Extension) =>
  run(
    ext,
    () => installExtension(ext.id, configToSend(ext, forms[ext.id], clearing[ext.id]), accounts[ext.id]),
    ext.kind === "mcp" ? "Pronto. Reinicie o Claude nas abas (/exit) pra ele ver o MCP." : "Pronto. Em até um minuto aparece na barra de status.",
  );

const uninstall = (ext: Extension) => run(ext, () => uninstallExtension(ext.id), "Removida.");

function stateLabel(ext: Extension) {
  if (!ext.installed) return "não instalada";
  if (ext.kind === "service") return ext.running ? "rodando" : "instalada, parada";
  const n = ext.accounts.filter((a) => a.installed).length;
  return `instalada em ${n} ${n === 1 ? "conta" : "contas"}`;
}

function toggleAccount(ext: Extension, id: string) {
  const list = accounts[ext.id];
  accounts[ext.id] = list.includes(id) ? list.filter((a) => a !== id) : [...list, id];
}

onMounted(load);
</script>

<template>
  <div class="exts">
    <p v-if="loadError" class="error">Não deu pra listar as extensões: {{ loadError }}</p>
    <p v-else-if="exts && !exts.length" class="hint">Este term7 veio sem extensões (build sem a pasta extensions/).</p>
    <div v-for="ext in exts ?? []" :key="ext.id" class="ext" :class="{ open: open === ext.id }">
      <button type="button" class="head" @click="open = open === ext.id ? null : ext.id">
        <span class="dot" :class="{ on: ext.installed && (ext.kind === 'mcp' || ext.running), off: ext.installed && ext.kind === 'service' && !ext.running }" />
        <span class="name">{{ ext.name }}</span>
        <span class="state">{{ stateLabel(ext) }}</span>
        <svg class="chev" viewBox="0 0 16 16" width="12" height="12"><path d="M4 6l4 4 4-4" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" /></svg>
      </button>

      <div v-if="open === ext.id" class="body">
        <p class="desc">
          {{ ext.description }}
          <a v-if="ext.link" href="#" @click.prevent="openUrl(ext.link!).catch(() => {})">Saiba mais</a>
        </p>

        <label v-for="f in ext.config" :key="f.key" class="field">
          <span>{{ f.label }}<b v-if="f.required"> *</b></span>
          <input
            v-model.trim="forms[ext.id][f.key]"
            :type="f.type === 'secret' ? 'password' : f.type === 'number' ? 'number' : 'text'"
            :placeholder="f.type === 'secret' && ext.secrets_set.includes(f.key) ? '•••••• (salvo; deixe vazio pra manter)' : (f.placeholder ?? f.default ?? '')"
            spellcheck="false"
            autocomplete="off"
            @keydown.enter.prevent="install(ext)"
          />
          <small>
            {{ f.help }}
            <a
              v-if="f.type === 'secret' && ext.secrets_set.includes(f.key) && !clearing[ext.id].has(f.key)"
              href="#"
              @click.prevent="clearing[ext.id].add(f.key)"
            >Apagar o salvo</a>
            <template v-else-if="clearing[ext.id].has(f.key)">Será apagado ao salvar.</template>
          </small>
        </label>

        <div v-if="ext.kind === 'mcp'" class="field">
          <span>Contas do Claude</span>
          <p v-if="!ext.accounts.length" class="hint">Nenhuma conta encontrada (~/.claude.json ou ~/.claude-*/.claude.json).</p>
          <label v-for="a in ext.accounts" :key="a.id" class="check">
            <input type="checkbox" :checked="accounts[ext.id].includes(a.id)" @change="toggleAccount(ext, a.id)" />
            <code>{{ a.label }}</code>
            <span v-if="a.installed" class="tag">instalado</span>
          </label>
        </div>

        <div class="actions">
          <span v-if="status[ext.id]" class="status" :class="{ error: !status[ext.id].ok }">{{ status[ext.id].text }}</span>
          <button v-if="ext.installed" type="button" class="secondary" :disabled="busy === ext.id" @click="uninstall(ext)">Remover</button>
          <button type="button" class="primary" :disabled="busy === ext.id" @click="install(ext)">
            {{ busy === ext.id ? "Aguarde…" : ext.installed ? "Salvar" : "Instalar" }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.exts {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 10px;
}
.ext {
  border: 1px solid var(--border);
  border-radius: 6px;
}
.head {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 8px 10px;
  border: 0;
  background: none;
  color: var(--fg);
  font: inherit;
  text-align: left;
  cursor: pointer;
}
.head:hover {
  background: var(--hover);
}
.name {
  font-weight: 600;
}
.state {
  flex: 1;
  color: var(--fg-muted);
  font-size: 12px;
}
.chev {
  color: var(--fg-muted);
  transition: transform 0.15s;
}
.open .chev {
  transform: rotate(180deg);
}
.dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--border);
}
.dot.on {
  background: var(--green);
}
.dot.off {
  background: var(--yellow);
}
.body {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 4px 10px 10px;
}
.desc {
  margin: 0;
  color: var(--fg-muted);
  font-size: 12.5px;
}
a {
  color: var(--accent);
}
.field {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 12.5px;
}
.field input:not([type="checkbox"]) {
  padding: 5px 8px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--bg);
  color: var(--fg);
  font: inherit;
  outline: none;
}
.field input:focus {
  border-color: var(--accent);
}
.field small,
.hint {
  margin: 0;
  color: var(--fg-muted);
  font-size: 11.5px;
}
.check {
  display: flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
}
.tag {
  color: var(--green);
  font-size: 11px;
}
.actions {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
}
.status {
  flex: 1;
  color: var(--green);
  font-size: 12px;
}
.status.error,
.error {
  color: var(--red);
}
.actions button {
  padding: 4px 12px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: none;
  color: var(--fg);
  font: inherit;
  font-size: 12px;
  cursor: pointer;
}
.actions button.primary {
  border-color: var(--accent);
  background: color-mix(in srgb, var(--accent) 20%, transparent);
}
.actions button:disabled {
  opacity: 0.5;
  cursor: default;
}
</style>
