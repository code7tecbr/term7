<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import StepItems from "./StepItems.vue";
import { chatInputs, chatSend, chatStart, chatStop, resultText, toolSummary } from "../lib/chat";
import { md, onLinkClick } from "../lib/markdown";
import type { AgentStep } from "../lib/pty";

/**
 * Modo chat (experimental): o Claude da aba sem a interface de terminal, com o
 * mesmo visual do popup dos agentes. Quem cuida do processo é este componente:
 * abre ao montar e encerra ao desmontar.
 */
const props = defineProps<{ tabId: number; ptyId: number; home: string | null; focused: boolean }>();
const emit = defineEmits<{ started: [id: number, resumeCommand: string]; failed: [message: string]; back: [] }>();

interface Permission {
  requestId: string;
  tool: string;
  input: Record<string, unknown>;
  summary: string;
  detail: string | null;
  suggestions: unknown[];
}

const steps = ref<AgentStep[]>([]);
/** Texto da resposta chegando aos poucos, antes de virar passo. */
const streaming = ref("");
const busy = ref(false);
const permissions = ref<Permission[]>([]);
const model = ref("");
const cost = ref<number | null>(null);
const exited = ref<{ code: number; stderr: string } | null>(null);
const starting = ref(true);
const draft = ref("");

const list = ref<HTMLDivElement>();
const input = ref<HTMLTextAreaElement>();
let chatId: number | null = null;
let cwd = "";
// tool_use_id -> posição do passo, pra encaixar o resultado que vem depois.
const toolAt = new Map<string, number>();
/** Quem rolou pra cima está lendo: só acompanha o fim se já estava lá. */
let follow = true;

async function scrollEnd() {
  if (!follow) return;
  await nextTick();
  list.value?.scrollTo({ top: list.value.scrollHeight });
}

function onScroll() {
  const el = list.value!;
  follow = el.scrollHeight - el.scrollTop - el.clientHeight < 60;
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any -- JSON do Claude, lido campo a campo
function onLine(raw: string) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let d: any;
  try {
    d = JSON.parse(raw);
  } catch {
    return;
  }
  // Mensagens dos subagentes também chegam aqui; elas ficam no menu "Agentes".
  if (d.parent_tool_use_id) return;
  switch (d.type) {
    case "system":
      if (d.subtype === "init") model.value = d.model ?? "";
      break;
    case "stream_event": {
      const ev = d.event;
      if (ev?.type === "message_start") busy.value = true;
      else if (ev?.type === "content_block_start" && ev.content_block?.type === "text") streaming.value = "";
      else if (ev?.type === "content_block_delta" && ev.delta?.type === "text_delta") {
        streaming.value += ev.delta.text;
        scrollEnd();
      }
      break;
    }
    case "assistant":
      for (const c of d.message?.content ?? []) {
        if (c.type === "text" && c.text?.trim()) {
          steps.value.push({ kind: "text", text: c.text });
          streaming.value = "";
        } else if (c.type === "tool_use") {
          const { summary, detail } = toolSummary(c.name, c.input ?? {}, cwd, props.home);
          toolAt.set(c.id, steps.value.length);
          steps.value.push({ kind: "tool", name: c.name, summary, detail, result: null, error: false });
        }
      }
      scrollEnd();
      break;
    case "user":
      for (const c of Array.isArray(d.message?.content) ? d.message.content : []) {
        if (c.type !== "tool_result") continue;
        const at = toolAt.get(c.tool_use_id);
        const step = at === undefined ? undefined : steps.value[at];
        if (step?.kind !== "tool") continue;
        step.result = resultText(c.content);
        step.error = !!c.is_error;
        toolAt.delete(c.tool_use_id);
      }
      break;
    case "control_request":
      if (d.request?.subtype === "can_use_tool") {
        const { summary, detail } = toolSummary(d.request.tool_name, d.request.input ?? {}, cwd, props.home);
        permissions.value.push({
          requestId: d.request_id,
          tool: d.request.display_name ?? d.request.tool_name,
          input: d.request.input ?? {},
          summary: d.request.description || summary,
          detail: detail ?? (d.request.description ? summary : null),
          suggestions: d.request.permission_suggestions ?? [],
        });
        scrollEnd();
      }
      break;
    case "control_cancel_request":
      permissions.value = permissions.value.filter((p) => p.requestId !== d.request_id);
      break;
    case "result":
      busy.value = false;
      streaming.value = "";
      if (typeof d.total_cost_usd === "number") cost.value = (cost.value ?? 0) + d.total_cost_usd;
      if (d.is_error && d.result) steps.value.push({ kind: "text", text: `**Erro:** ${d.result}` });
      scrollEnd();
      break;
  }
}

function send(message: unknown) {
  if (chatId === null) return Promise.resolve();
  return chatSend(chatId, message).catch((err) => steps.value.push({ kind: "text", text: `**Não consegui enviar:** ${err}` }));
}

async function submit() {
  const text = draft.value.trim();
  if (!text || chatId === null || exited.value) return;
  draft.value = "";
  follow = true;
  steps.value.push({ kind: "prompt", text });
  busy.value = true;
  scrollEnd();
  await send({ type: "user", message: { role: "user", content: text } });
}

/** Igual ao Esc do terminal: para a resposta atual. */
function interrupt() {
  if (!busy.value) return;
  send({ type: "control_request", request_id: crypto.randomUUID(), request: { subtype: "interrupt" } });
}

function answer(p: Permission, choice: "allow" | "always" | "deny") {
  permissions.value = permissions.value.filter((x) => x !== p);
  const response =
    choice === "deny"
      ? { behavior: "deny", message: "O usuário negou pelo term7." }
      : { behavior: "allow", updatedInput: p.input, ...(choice === "always" ? { updatedPermissions: p.suggestions } : {}) };
  send({ type: "control_response", response: { subtype: "success", request_id: p.requestId, response } });
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
    e.preventDefault();
    submit();
  } else if (e.key === "Escape" && busy.value) {
    e.preventDefault();
    interrupt();
  }
}

/** A caixa cresce com o texto, até um limite. */
function autosize() {
  const el = input.value;
  if (!el) return;
  el.style.height = "auto";
  el.style.height = `${Math.min(el.scrollHeight, 220)}px`;
}
watch(draft, () => nextTick(autosize));

function insert(text: string) {
  const el = input.value;
  if (!el) return void (draft.value += text);
  const [a, b] = [el.selectionStart, el.selectionEnd];
  draft.value = draft.value.slice(0, a) + text + draft.value.slice(b);
  nextTick(() => {
    el.focus();
    el.setSelectionRange(a + text.length, a + text.length);
  });
}

watch(
  () => props.focused,
  (f) => f && nextTick(() => input.value?.focus()),
);

const modelLabel = computed(() => {
  const m = /^claude-([a-z]+)-(\d+)-(\d+)/.exec(model.value);
  return m ? `${m[1]} ${m[2]}.${m[3]}` : model.value;
});

onMounted(async () => {
  chatInputs.set(props.tabId, insert);
  try {
    const started = await chatStart(props.ptyId, onLine, (code, stderr) => {
      exited.value = { code, stderr: stderr.trim() };
      busy.value = false;
      permissions.value = [];
      chatId = null;
    });
    chatId = started.id;
    cwd = started.cwd;
    steps.value = started.history;
    emit("started", started.id, started.resume_command);
    scrollEnd();
  } catch (err) {
    emit("failed", String(err));
  } finally {
    starting.value = false;
    if (props.focused) nextTick(() => input.value?.focus());
  }
});

onBeforeUnmount(() => {
  chatInputs.delete(props.tabId);
  if (chatId !== null) chatStop(chatId).catch(() => {});
});
</script>

<template>
  <div class="chat">
    <div ref="list" class="list" @scroll="onScroll">
      <div class="inner">
        <p v-if="starting" class="empty">Abrindo a conversa…</p>
        <p v-else-if="!steps.length && !exited" class="empty">Escreva embaixo pra começar.</p>
        <StepItems :steps="steps" prompt-label="Você" />
        <!-- eslint-disable-next-line vue/no-v-html -- sanitizado com DOMPurify -->
        <div v-if="streaming" class="text markdown say streaming" @click="onLinkClick" v-html="md(streaming)" />
        <div v-for="p in permissions" :key="p.requestId" class="permission">
          <div class="perm-head">
            <span class="perm-label">Permissão</span>
            <span class="tname">{{ p.tool }}</span>
            <span class="perm-summary">{{ p.summary }}</span>
          </div>
          <pre v-if="p.detail" class="perm-detail">{{ p.detail }}</pre>
          <div class="perm-actions">
            <button class="allow" @click="answer(p, 'allow')">Permitir</button>
            <button v-if="p.suggestions.length" class="ghost" title="Permite e grava a regra, como o “não perguntar de novo” do terminal" @click="answer(p, 'always')">
              Sempre permitir
            </button>
            <button class="ghost deny" @click="answer(p, 'deny')">Negar</button>
          </div>
        </div>
        <div v-if="busy && !streaming && !permissions.length" class="thinking"><span class="pulse" />Pensando…</div>
        <div v-if="exited" class="exited">
          <p>
            O Claude do chat fechou (código {{ exited.code }}).<template v-if="exited.stderr"> Última mensagem de erro:</template>
          </p>
          <pre v-if="exited.stderr">{{ exited.stderr }}</pre>
          <button class="allow" @click="emit('back')">Voltar ao terminal</button>
        </div>
      </div>
    </div>

    <form class="composer" @submit.prevent="submit">
      <div class="box" :class="{ disabled: !!exited || starting }">
        <textarea
          ref="input"
          v-model="draft"
          rows="1"
          :placeholder="exited ? 'O chat fechou.' : 'Mensagem pro Claude (Enter envia, Shift+Enter quebra a linha)'"
          :disabled="!!exited || starting"
          spellcheck="false"
          @keydown="onKeydown"
        />
        <div class="bar">
          <span class="meta">
            <template v-if="modelLabel">{{ modelLabel }}</template>
            <template v-if="cost !== null"> · US$ {{ cost.toFixed(2) }} nesta conversa</template>
          </span>
          <button v-if="busy" type="button" class="ghost" title="Para a resposta (Esc)" @click="interrupt">Parar</button>
          <button type="submit" class="send" :disabled="!draft.trim() || !!exited || starting">Enviar</button>
        </div>
      </div>
    </form>
  </div>
</template>

<style scoped>
.chat {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  background: var(--bg);
  font-size: 13.5px;
}
.list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
}
/* Coluna de leitura no meio, como num site de chat. */
.inner {
  display: flex;
  flex-direction: column;
  gap: 10px;
  max-width: 860px;
  margin: 0 auto;
  padding: 20px 20px 12px;
}
.empty {
  margin: 40px auto;
  color: var(--fg-muted);
}
.text {
  overflow-wrap: anywhere;
  line-height: 1.55;
  user-select: text;
}
.streaming :deep(> :last-child)::after {
  content: "▍";
  margin-left: 2px;
  color: var(--claude);
  animation: pulse 1s ease-in-out infinite;
}
.markdown :deep(> :first-child) {
  margin-top: 0;
}
.markdown :deep(> :last-child) {
  margin-bottom: 0;
}
.markdown :deep(p),
.markdown :deep(ul),
.markdown :deep(ol),
.markdown :deep(pre) {
  margin: 0 0 0.6em;
}
.markdown :deep(code) {
  padding: 0.1em 0.35em;
  border-radius: 4px;
  background: var(--bg-elevated);
  font-family: var(--font-mono);
  font-size: 0.88em;
}
.markdown :deep(pre) {
  padding: 8px 10px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--bg-elevated);
  overflow-x: auto;
}
.markdown :deep(pre code) {
  padding: 0;
  background: none;
}
.thinking {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--fg-muted);
  font-size: 12px;
}
.pulse {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--claude);
  animation: pulse 1.2s ease-in-out infinite;
}
.permission {
  padding: 10px 12px;
  border: 1px solid color-mix(in srgb, var(--yellow) 45%, transparent);
  border-radius: 8px;
  background: color-mix(in srgb, var(--yellow) 6%, transparent);
}
.perm-head {
  display: flex;
  align-items: baseline;
  gap: 8px;
  min-width: 0;
}
.perm-label {
  color: var(--yellow);
  font-size: 11px;
  font-weight: 600;
}
.tname {
  color: var(--purple);
  font-weight: 600;
  font-size: 12px;
}
.perm-summary {
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
  font-size: 12.5px;
}
.perm-detail,
.exited pre {
  max-height: 160px;
  margin: 8px 0 0;
  padding: 8px 10px;
  overflow: auto;
  border-radius: 6px;
  background: var(--bg-elevated);
  font-family: var(--font-mono);
  font-size: 11.5px;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}
.perm-actions {
  display: flex;
  gap: 8px;
  margin-top: 10px;
}
button {
  height: 28px;
  padding: 0 12px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: none;
  color: var(--fg);
  font: inherit;
  font-size: 12px;
  cursor: pointer;
}
button:hover:not(:disabled) {
  background: var(--hover);
}
button:disabled {
  opacity: 0.45;
  cursor: default;
}
button.allow,
button.send {
  border-color: transparent;
  background: var(--claude);
  color: #1a1b26;
  font-weight: 600;
}
button.allow:hover:not(:disabled),
button.send:hover:not(:disabled) {
  background: color-mix(in srgb, var(--claude) 85%, white);
}
button.deny {
  color: var(--red);
}
.exited {
  padding: 10px 12px;
  border: 1px solid color-mix(in srgb, var(--red) 40%, transparent);
  border-radius: 8px;
}
.exited p {
  margin: 0 0 4px;
}
.exited button {
  margin-top: 10px;
}
.composer {
  flex-shrink: 0;
  padding: 8px 20px 14px;
}
.box {
  max-width: 860px;
  margin: 0 auto;
  border: 1px solid var(--border);
  border-radius: 10px;
  background: var(--bg-elevated);
}
.box:focus-within {
  border-color: color-mix(in srgb, var(--claude) 55%, var(--border));
}
.box.disabled {
  opacity: 0.6;
}
textarea {
  display: block;
  width: 100%;
  min-height: 42px;
  max-height: 220px;
  padding: 11px 12px 4px;
  border: 0;
  outline: 0;
  resize: none;
  background: none;
  color: var(--fg);
  font: inherit;
  line-height: 1.5;
}
.bar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 8px 8px 12px;
}
.meta {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  color: var(--fg-muted);
  font-size: 11px;
  white-space: nowrap;
  text-overflow: ellipsis;
}
@keyframes pulse {
  50% {
    opacity: 0.3;
  }
}
@media (prefers-reduced-motion: reduce) {
  .pulse,
  .streaming :deep(> :last-child)::after {
    animation: none;
  }
}
</style>
