<script setup lang="ts">
import { computed } from "vue";
import type { Agent } from "../lib/pty";

/** Barra de cima: o que é do Claude da aba ativa (agentes e MCPs). A de baixo fica com pasta, git e PR. */
const props = defineProps<{
  title: string;
  claude: boolean;
  agents: Agent[] | null;
  agentsOpen: boolean;
  mcp: { servers: { status: string }[] | null; loading: boolean } | null;
  mcpOpen: boolean;
}>();
defineEmits<{ toggleAgents: []; toggleMcp: [] }>();

const running = computed(() => props.agents?.filter((a) => a.status === "running").length ?? 0);

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

    <div v-if="claude" class="right" @mousedown.prevent>
      <button
        v-if="agents?.length"
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
      <button class="btn" data-mcp-toggle :class="{ open: mcpOpen }" title="Servidores MCP do Claude desta aba" @click="$emit('toggleMcp')">
        <span class="label">MCP</span>
        <template v-if="mcp?.servers">
          <span class="ok">{{ mcpCounts.ok }}/{{ mcpCounts.total }}</span>
          <span v-if="mcpCounts.auth" class="auth" title="Precisam de autenticação">⚠ {{ mcpCounts.auth }}</span>
          <span v-if="mcpCounts.failed" class="failed" title="Falharam">✕ {{ mcpCounts.failed }}</span>
        </template>
        <span v-else-if="mcp?.loading" class="muted">…</span>
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
  .pulse {
    animation: none;
  }
}
</style>
