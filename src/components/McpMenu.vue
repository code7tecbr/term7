<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import type { McpServer } from "../lib/pty";

/** Servidores MCP do Claude da aba ativa (os do projeto, ou os globais se ele não tiver; via `claude mcp get`). */
const props = defineProps<{
  servers: McpServer[] | null;
  loading: boolean;
  error: string | null;
  /** Pasta de configuração do Claude da aba (ex.: ~/.claude-code7). */
  configDir: string | null;
  checkedAt: number | null;
  /** Mostrando os globais em vez dos do projeto. */
  global: boolean;
}>();
const emit = defineEmits<{ close: []; refresh: []; authenticate: []; scope: [global: boolean] }>();

const box = ref<HTMLDivElement>();

const ORDER = { needs_auth: 0, failed: 1, unknown: 2, connected: 3 } as const;
const LABEL = {
  connected: "Conectado",
  needs_auth: "Precisa de autenticação",
  failed: "Falhou",
  unknown: "Desconhecido",
} as const;

// Os que pedem ação primeiro.
const sorted = computed(() =>
  [...(props.servers ?? [])].sort((a, b) => ORDER[a.status] - ORDER[b.status] || a.name.localeCompare(b.name)),
);
const needsAuth = computed(() => sorted.value.some((s) => s.status === "needs_auth"));

const checked = computed(() =>
  props.checkedAt ? new Date(props.checkedAt).toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" }) : null,
);

function onPointerDown(e: PointerEvent) {
  const target = e.target as HTMLElement;
  // O botão MCP da barra já alterna o menu: fechar aqui faria ele reabrir no click.
  if (box.value?.contains(target) || target.closest("[data-mcp-toggle]")) return;
  emit("close");
}
function onKey(e: KeyboardEvent) {
  if (e.key !== "Escape") return;
  e.preventDefault();
  e.stopPropagation();
  emit("close");
}

onMounted(() => {
  window.addEventListener("pointerdown", onPointerDown, true);
  window.addEventListener("keydown", onKey, true);
});
onBeforeUnmount(() => {
  window.removeEventListener("pointerdown", onPointerDown, true);
  window.removeEventListener("keydown", onKey, true);
});
</script>

<template>
  <div ref="box" class="mcp" @mousedown.prevent>
    <div class="head">
      <span class="title">{{ global ? "MCPs globais" : "MCPs do projeto" }}</span>
      <span class="config" :title="configDir ?? ''">{{ configDir }}</span>
      <button class="refresh" :class="{ spinning: loading }" title="Verificar de novo" :disabled="loading" @click="emit('refresh')">
        <svg viewBox="0 0 16 16" width="14" height="14"><path d="M13 8a5 5 0 1 1-1.5-3.5M13 3v2.5h-2.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" /></svg>
      </button>
    </div>

    <div v-if="error" class="error">{{ error }}</div>
    <ul>
      <li v-if="loading && !servers" class="muted">Verificando os servidores…</li>
      <li v-else-if="servers && !servers.length" class="muted">
        {{ global ? "Nenhum MCP global configurado." : "Nenhum MCP neste projeto (.mcp.json ou escopo local)." }}
      </li>
      <li v-for="s in sorted" :key="s.name" :class="s.status" :title="s.detail ?? LABEL[s.status]">
        <span class="dot" />
        <div class="text">
          <span class="name">{{ s.name }}</span>
          <span class="target">{{ s.detail ?? s.target }}</span>
        </div>
        <span class="status">{{ LABEL[s.status] }}</span>
      </li>
    </ul>

    <div class="foot">
      <button v-if="needsAuth" class="auth" title="Digita /mcp no Claude: é lá que ele faz o login dos servidores" @click="emit('authenticate')">
        Autenticar no Claude (/mcp)
      </button>
      <button
        class="scope"
        :title="global ? 'Volta pros MCPs do projeto' : 'Checa os MCPs do usuário (valem pra todos os projetos; demora mais)'"
        :disabled="loading"
        @click="emit('scope', !global)"
      >
        {{ global ? "Ver os do projeto" : "Ver os globais" }}
      </button>
      <span class="muted">{{ loading ? "Verificando…" : checked ? `Verificado às ${checked}` : "" }}</span>
    </div>
  </div>
</template>

<style scoped>
.mcp {
  position: fixed;
  right: 10px;
  top: 32px;
  z-index: 150;
  display: flex;
  flex-direction: column;
  width: min(440px, calc(100vw - 20px));
  max-height: min(480px, 70vh);
  background: var(--bg-elevated);
  border: 1px solid var(--border);
  border-radius: 10px;
  box-shadow: 0 12px 32px rgb(0 0 0 / 0.45);
  font-size: 12.5px;
  overflow: hidden;
  user-select: none;
}
.head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 10px 6px 12px;
}
.title {
  font-weight: 600;
}
.config {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  color: var(--fg-muted);
  font-size: 11px;
  white-space: nowrap;
  text-overflow: ellipsis;
}
.refresh {
  display: grid;
  place-items: center;
  width: 24px;
  height: 24px;
  border: 0;
  border-radius: 5px;
  background: none;
  color: var(--fg-muted);
  cursor: pointer;
}
.refresh:hover:not(:disabled) {
  background: var(--hover);
  color: var(--fg);
}
.refresh.spinning svg {
  animation: spin 0.9s linear infinite;
}
.error {
  padding: 4px 12px 8px;
  color: var(--red);
}
ul {
  flex: 1;
  margin: 0;
  padding: 2px 6px 6px;
  list-style: none;
  overflow-y: auto;
}
li {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 6px 8px;
  border-radius: 6px;
}
li:hover {
  background: var(--hover);
}
li.muted {
  color: var(--fg-muted);
}
.dot {
  flex-shrink: 0;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--fg-muted);
}
li.connected .dot {
  background: var(--green);
}
li.needs_auth .dot {
  background: var(--yellow);
}
li.failed .dot {
  background: var(--red);
}
.text {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-width: 0;
}
.name {
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}
.target {
  overflow: hidden;
  color: var(--fg-muted);
  font-size: 11px;
  white-space: nowrap;
  text-overflow: ellipsis;
}
.status {
  flex-shrink: 0;
  color: var(--fg-muted);
  font-size: 11px;
}
li.needs_auth .status {
  color: var(--yellow);
}
li.failed .status {
  color: var(--red);
}
.foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 8px 12px;
  border-top: 1px solid var(--border);
}
.auth {
  padding: 4px 10px;
  border: 1px solid color-mix(in srgb, var(--yellow) 50%, transparent);
  border-radius: 6px;
  background: color-mix(in srgb, var(--yellow) 12%, transparent);
  color: var(--yellow);
  font: inherit;
  cursor: pointer;
}
.auth:hover {
  background: color-mix(in srgb, var(--yellow) 22%, transparent);
}
.muted {
  color: var(--fg-muted);
  font-size: 11px;
}
.scope {
  padding: 4px 10px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: none;
  color: var(--fg);
  font: inherit;
  cursor: pointer;
}
.scope:hover:not(:disabled) {
  background: var(--hover);
}
.scope:disabled {
  opacity: 0.5;
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
