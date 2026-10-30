<script setup lang="ts">
import { ref } from "vue";
import type { AgentStep } from "../lib/pty";
import { md, onLinkClick } from "../lib/markdown";

/**
 * Passos de uma conversa do Claude: mensagens do usuário, textos em Markdown e
 * ferramentas que abrem e fecham. Usado no popup dos agentes.
 */
defineProps<{ steps: AgentStep[]; promptLabel: string }>();

// Ferramentas abertas, pela posição do passo (a lista só cresce no fim).
const open = ref(new Set<number>());

function toggle(i: number) {
  const s = new Set(open.value);
  if (!s.delete(i)) s.add(i);
  open.value = s;
}
</script>

<template>
  <template v-for="(s, i) in steps" :key="i">
    <div v-if="s.kind === 'prompt'" class="prompt">
      <span class="label">{{ promptLabel }}</span>
      <!-- eslint-disable-next-line vue/no-v-html -- sanitizado com DOMPurify -->
      <div class="text markdown" @click="onLinkClick" v-html="md(s.text)" />
    </div>
    <!-- eslint-disable-next-line vue/no-v-html -- sanitizado com DOMPurify -->
    <div v-else-if="s.kind === 'text'" class="text markdown say" @click="onLinkClick" v-html="md(s.text)" />
    <div v-else class="tool" :class="{ error: s.error, pending: s.result === null }">
      <button class="call" :title="s.detail ?? s.summary" @click="toggle(i)">
        <span class="tname">{{ s.name }}</span>
        <span class="summary" :class="{ plain: s.detail }">{{ s.summary }}</span>
        <span class="state">{{ s.result === null ? "rodando…" : s.error ? "erro" : open.has(i) ? "▾" : "▸" }}</span>
      </button>
      <template v-if="open.has(i)">
        <pre v-if="s.detail" class="detail">{{ s.detail }}</pre>
        <pre class="result">{{ s.result === null ? "(ainda rodando)" : s.result || "(sem saída)" }}</pre>
      </template>
    </div>
  </template>
</template>

<style scoped>
.text {
  overflow-wrap: anywhere;
  line-height: 1.55;
  user-select: text;
}
/* Margens de bloco somem nas pontas: cada passo já tem o seu espaçamento. */
.markdown :deep(> :first-child) {
  margin-top: 0;
}
.markdown :deep(> :last-child) {
  margin-bottom: 0;
}
.markdown :deep(p),
.markdown :deep(ul),
.markdown :deep(ol),
.markdown :deep(pre),
.markdown :deep(blockquote),
.markdown :deep(table) {
  margin: 0 0 0.6em;
}
.markdown :deep(ul),
.markdown :deep(ol) {
  padding-left: 1.4em;
}
.markdown :deep(h1),
.markdown :deep(h2),
.markdown :deep(h3),
.markdown :deep(h4) {
  margin: 0.9em 0 0.4em;
  font-size: 1em;
  line-height: 1.3;
}
.markdown :deep(h1),
.markdown :deep(h2) {
  font-size: 1.1em;
}
.markdown :deep(a) {
  color: var(--accent);
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
.markdown :deep(blockquote) {
  padding: 0 0.9em;
  border-left: 3px solid var(--border);
  color: var(--fg-muted);
}
/* Tabela larga rola sozinha, sem alargar o popup. */
.markdown :deep(table) {
  display: block;
  max-width: 100%;
  overflow-x: auto;
  border-collapse: collapse;
  font-size: 12px;
}
.markdown :deep(th),
.markdown :deep(td) {
  padding: 4px 8px;
  border: 1px solid var(--border);
}
.markdown :deep(hr) {
  border: 0;
  border-top: 1px solid var(--border);
}
.prompt {
  padding: 8px 10px;
  border-left: 2px solid var(--claude);
  border-radius: 4px;
  background: var(--bg-elevated);
}
.prompt .markdown :deep(code),
.prompt .markdown :deep(pre) {
  background: var(--bg);
}
.prompt .text {
  max-height: 180px;
  overflow-y: auto;
  color: var(--fg-muted);
}
.label {
  display: block;
  margin-bottom: 4px;
  color: var(--claude);
  font-size: 11px;
  font-weight: 600;
}
.tool {
  border: 1px solid var(--border);
  border-radius: 6px;
}
.call {
  display: flex;
  align-items: baseline;
  gap: 8px;
  width: 100%;
  padding: 5px 9px;
  border: 0;
  background: none;
  color: var(--fg);
  font: inherit;
  font-size: 12px;
  text-align: left;
  cursor: pointer;
}
.call:hover {
  background: var(--hover);
}
.tname {
  flex-shrink: 0;
  color: var(--purple);
  font-weight: 600;
}
.summary {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  color: var(--fg-muted);
  font-family: var(--font-mono);
  white-space: nowrap;
  text-overflow: ellipsis;
}
/* Descrição do Bash é frase, não código. */
.summary.plain {
  font-family: inherit;
}
.state {
  flex-shrink: 0;
  color: var(--fg-muted);
  font-size: 11px;
}
.tool.pending .state {
  color: var(--accent);
}
.tool.error {
  border-color: rgb(247 118 142 / 0.4);
}
.tool.error .state {
  color: var(--red);
}
.detail {
  margin: 0;
  padding: 8px 10px;
  max-height: 160px;
  overflow: auto;
  border-top: 1px solid var(--border);
  color: var(--fg);
  font-family: var(--font-mono);
  font-size: 11.5px;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}
.result {
  max-height: 240px;
  margin: 0;
  padding: 8px 10px;
  overflow: auto;
  border-top: 1px solid var(--border);
  color: var(--fg-muted);
  font-family: var(--font-mono);
  font-size: 11.5px;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}
</style>
