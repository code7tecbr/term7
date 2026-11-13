<script setup lang="ts">
import { computed } from "vue";
import type { Agent, BgTask, ClaudeUsage, UsageWindow } from "../lib/pty";

/** Barra de cima: o que é do Claude da aba ativa (agentes e MCPs). A de baixo fica com pasta, git e PR. */
const props = defineProps<{
  title: string;
  claude: boolean;
  agents: Agent[] | null;
  agentsOpen: boolean;
  tasks: BgTask[] | null;
  tasksOpen: boolean;
  mcp: { servers: { status: string }[] | null; loading: boolean } | null;
  mcpOpen: boolean;
  /** Ditado: parado, gravando (com os segundos) ou esperando a transcrição. */
  voice: "idle" | "recording" | "transcribing" | "speaking";
  voiceSecs: number;
  anon: boolean;
  /** Uso da assinatura do Claude da aba; null = sem Claude ou ainda lendo. */
  usage: ClaudeUsage | null;
}>();
defineEmits<{ toggleAgents: []; toggleTasks: []; toggleMcp: []; voice: []; attach: []; settings: []; toggleAnon: []; setupUsage: [] }>();

const clock = computed(() => `${Math.floor(props.voiceSecs / 60)}:${String(props.voiceSecs % 60).padStart(2, "0")}`);

/** "five_hour" -> "5h", "seven_day_fable" -> "7d Fable". */
function usageLabel(key: string) {
  const m = /^(five_hour|seven_day)(?:_(.+))?$/.exec(key);
  if (!m) return key.replace(/_/g, " ");
  const base = m[1] === "five_hour" ? "5h" : "7d";
  return m[2] ? `${base} ${m[2].charAt(0).toUpperCase()}${m[2].slice(1).replace(/_/g, " ")}` : base;
}

function untilReset(w: UsageWindow) {
  if (!w.resets_at) return "";
  const s = w.resets_at - Date.now() / 1000;
  if (s <= 0) return " · já reiniciou";
  const m = Math.round(s / 60);
  if (m < 60) return ` · reinicia em ${m}min`;
  const h = Math.floor(m / 60);
  return h < 24 ? ` · reinicia em ${h}h${String(m % 60).padStart(2, "0")}` : ` · reinicia em ${Math.floor(h / 24)}d ${h % 24}h`;
}

/** 1000000 -> "1M", 200000 -> "200 mil". */
const tokens = (n: number) => (n >= 1_000_000 ? `${+(n / 1_000_000).toFixed(1)}M` : `${Math.round(n / 1000)} mil`);

const usageTitle = computed(() => {
  const u = props.usage;
  if (!u) return "";
  const ctx = u.context;
  const lines = ctx ? [`Contexto desta sessão: ${Math.round(ctx.used)}% usado${ctx.size ? ` de ${tokens(ctx.size)} tokens` : ""}`] : [];
  lines.push(...u.windows.map((w) => `${usageLabel(w.key)}: ${Math.round(w.used)}% usado${untilReset(w)}`));
  if (u.updated) {
    const ago = Math.max(0, Math.round((Date.now() / 1000 - u.updated) / 60));
    lines.push(ago < 1 ? "Atualizado agora" : `Atualizado há ${ago}min (muda a cada resposta do Claude)`);
  }
  return lines.join("\n");
});

/** Cor do número: verde, verde-amarelo, amarelo, laranja e vermelho conforme o limite enche. */
const level = (used: number) => (used >= 90 ? "l5" : used >= 70 ? "l4" : used >= 50 ? "l3" : used >= 30 ? "l2" : "l1");

const running = computed(() => props.agents?.filter((a) => a.status === "running").length ?? 0);
const tasksRunning = computed(() => props.tasks?.filter((t) => t.status === "running").length ?? 0);

const mcpCounts = computed(() => {
  const servers = props.mcp?.servers ?? [];
  return {
    total: servers.length,
    ok: servers.filter((s) => s.status === "connected").length,
    auth: servers.filter((s) => s.status === "needs_auth").length,
    failed: servers.filter((s) => s.status === "failed").length,
  };
});
</script>

<template>
  <header class="topbar">
    <span class="title" :title="title">{{ title }}</span>

    <div class="right" @mousedown.prevent>
      <button
        v-if="claude && agents?.length"
        class="btn"
        data-agents-toggle
        :class="{ open: agentsOpen }"
        title="Subagentes do Claude desta aba"
        @click="$emit('toggleAgents')"
      >
        <span class="label">Agentes</span>
        <span v-if="running" class="running"><span class="pulse" />{{ running }} rodando</span>
        <span v-else class="muted">{{ agents.length }}</span>
      </button>
      <button
        v-if="claude && tasks?.length"
        class="btn"
        data-tasks-toggle
        :class="{ open: tasksOpen }"
        title="Comandos que o Claude desta aba rodou em segundo plano"
        @click="$emit('toggleTasks')"
      >
        <span class="label">Tarefas</span>
        <span v-if="tasksRunning" class="running"><span class="pulse" />{{ tasksRunning }} rodando</span>
        <span v-else class="muted">{{ tasks.length }}</span>
      </button>
      <button v-if="claude" class="btn" data-mcp-toggle :class="{ open: mcpOpen }" title="Servidores MCP do Claude desta aba" @click="$emit('toggleMcp')">
        <span class="label">MCP</span>
        <template v-if="mcp?.servers">
          <span class="ok">{{ mcpCounts.ok }}/{{ mcpCounts.total }}</span>
          <span v-if="mcpCounts.auth" class="auth" title="Precisam de autenticação">⚠ {{ mcpCounts.auth }}</span>
          <span v-if="mcpCounts.failed" class="failed" title="Falharam">✕ {{ mcpCounts.failed }}</span>
        </template>
        <span v-else-if="mcp?.loading" class="muted">…</span>
      </button>
      <template v-if="claude && usage">
        <button
          v-if="!usage.installed"
          class="btn"
          :title="`Mostra aqui o uso da assinatura (contexto, 5h, 7d e os limites por modelo, como o do Fable). Coloca o term7 na frente da linha de status em ${usage.settings_path}; a sua linha de status continua igual. Dá pra desfazer nas Configurações.`"
          @click="$emit('setupUsage')"
        >
          <span class="muted">Uso</span><span class="label">ativar</span>
        </button>
        <span v-else-if="usage.windows.length || usage.context" class="usage" :title="usageTitle">
          <svg class="spark" viewBox="0 0 16 16" width="12" height="12" aria-hidden="true"><path d="M8 1v14M1 8h14M3.05 3.05l9.9 9.9M12.95 3.05l-9.9 9.9M4.6 1.8l2.3 4.4M11.4 14.2l-2.3-4.4M14.2 4.6 9.8 6.9M1.8 11.4l4.4-2.3" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" /></svg>
          <span v-if="usage.context" class="win ctx" :class="level(usage.context.used)">
            ctx <b>{{ Math.round(usage.context.used) }}%</b>
          </span>
          <span v-for="w in usage.windows" :key="w.key" class="win" :class="level(w.used)">
            {{ usageLabel(w.key) }} <b>{{ Math.round(w.used) }}%</b>
          </span>
        </span>
        <span v-else class="usage muted" title="O Claude manda os números junto com a linha de status: aparecem depois da próxima resposta dele (numa sessão aberta depois de ativar)">Uso …</span>
      </template>
      <span class="sep" />
      <button class="btn icon" title="Anexar arquivos: cola os caminhos no terminal ativo (ou arraste os arquivos pra ele)" @click="$emit('attach')">
        <svg viewBox="0 0 16 16" width="14" height="14"><path d="M10.5 4.5 5.6 9.4a1.4 1.4 0 0 0 2 2l5-5a2.8 2.8 0 0 0-4-4l-5 5a4.2 4.2 0 0 0 6 6l4.4-4.4" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" /></svg>
      </button>
      <button
        class="btn icon mic"
        :class="voice"
        :disabled="voice === 'transcribing'"
        :title="
          voice === 'recording'
            ? 'Parar e transcrever'
            : voice === 'transcribing'
              ? 'Transcrevendo…'
              : voice === 'speaking'
                ? 'Lendo a resposta do Claude: clique pra parar'
                : 'Ditar: clique, fale e clique de novo (termine com “enviar” pra mandar)'
        "
        @click="$emit('voice')"
      >
        <svg viewBox="0 0 16 16" width="14" height="14"><rect x="5.5" y="1.75" width="5" height="8" rx="2.5" fill="none" stroke="currentColor" stroke-width="1.3" /><path d="M3.5 7.5a4.5 4.5 0 0 0 9 0M8 12v2.25" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" /></svg>
        <span v-if="voice === 'recording'">{{ clock }}</span>
        <span v-else-if="voice === 'transcribing'">Transcrevendo…</span>
        <span v-else-if="voice === 'speaking'">Parar voz</span>
      </button>
      <button
        class="btn icon anon-safe"
        :class="{ 'anon-on': anon }"
        :title="anon ? 'Desligar o modo anônimo' : 'Modo anônimo: embaralha todo o texto da tela pra gravar sem expor nada'"
        @click="$emit('toggleAnon')"
      >
        <svg viewBox="0 0 16 16" width="14" height="14"><path d="M1.5 8s2.4-4.5 6.5-4.5S14.5 8 14.5 8 12.1 12.5 8 12.5 1.5 8 1.5 8z" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" /><circle cx="8" cy="8" r="2" fill="none" stroke="currentColor" stroke-width="1.3" /><path v-if="anon" d="M2.5 13.5l11-11" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" /></svg>
        <span v-if="anon">Anônimo</span>
      </button>
      <button class="btn icon" title="Configurações" @click="$emit('settings')">
        <svg viewBox="0 0 16 16" width="14" height="14"><circle cx="8" cy="8" r="2" fill="none" stroke="currentColor" stroke-width="1.3" /><path d="M8 1.5v1.8M8 12.7v1.8M14.5 8h-1.8M3.3 8H1.5M12.6 3.4l-1.3 1.3M4.7 11.3l-1.3 1.3M12.6 12.6l-1.3-1.3M4.7 4.7 3.4 3.4" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" /></svg>
      </button>
    </div>
  </header>
</template>

<style scoped>
.topbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  flex-shrink: 0;
  height: 26px;
  padding: 0 10px;
  background: var(--bg-elevated);
  border-bottom: 1px solid var(--border);
  color: var(--fg-muted);
  font-size: 12px;
  user-select: none;
}
.title {
  min-width: 0;
  overflow: hidden;
  color: var(--fg);
  white-space: nowrap;
  text-overflow: ellipsis;
}
.right {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
}
.btn {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 20px;
  padding: 0 8px;
  border: 0;
  border-radius: 5px;
  background: none;
  color: inherit;
  font: inherit;
  cursor: pointer;
}
.btn:hover,
.btn.open {
  background: var(--hover);
  color: var(--fg);
}
.btn.icon {
  padding: 0 6px;
}
.btn:disabled {
  cursor: default;
}
.usage {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 6px;
  font-variant-numeric: tabular-nums;
  cursor: default;
}
.win b {
  color: var(--fg);
  font-weight: 600;
}
.win.ctx {
  padding-right: 8px;
  border-right: 1px solid var(--border);
}
.usage .spark {
  flex-shrink: 0;
  margin-right: -2px;
  color: var(--claude);
}
.win.l1 b {
  color: var(--green);
}
.win.l2 b {
  color: color-mix(in srgb, var(--green) 50%, var(--yellow));
}
.win.l3 b {
  color: var(--yellow);
}
.win.l4 b {
  color: var(--orange);
}
.win.l5 b {
  color: var(--red);
}
.mic.recording {
  background: color-mix(in srgb, var(--red) 16%, transparent);
  color: var(--red);
}
.mic.recording svg {
  animation: pulse 1.2s ease-in-out infinite;
}
.mic.transcribing {
  color: var(--accent);
}
.mic.speaking {
  background: color-mix(in srgb, var(--accent) 16%, transparent);
  color: var(--accent);
}
.anon-on {
  background: color-mix(in srgb, var(--purple) 18%, transparent);
  color: var(--purple);
}
.sep {
  width: 1px;
  height: 14px;
  margin: 0 4px;
  background: var(--border);
}
.label {
  font-weight: 600;
  letter-spacing: 0.04em;
}
.running {
  display: flex;
  align-items: center;
  gap: 5px;
  color: var(--accent);
}
.pulse {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--accent);
  animation: pulse 1.2s ease-in-out infinite;
}
.ok {
  color: var(--green);
}
.auth {
  color: var(--yellow);
}
.failed {
  color: var(--red);
}
.muted {
  color: var(--fg-muted);
}
@keyframes pulse {
  50% {
    opacity: 0.3;
  }
}
@media (prefers-reduced-motion: reduce) {
  .pulse,
  .mic.recording svg {
    animation: none;
  }
}
</style>
