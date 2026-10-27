<script setup lang="ts">
import { computed } from "vue";
import { repoName, type GitStatus } from "../lib/git";
import { prettyPath } from "../state/tabs";

const props = defineProps<{
  cwd: string | null;
  home: string | null;
  git: GitStatus | null;
  diffOpen: boolean;
  panelOpen: boolean;
  pickerOpen: boolean;
}>();
defineEmits<{ toggleDiff: []; togglePanel: []; pickBranch: [] }>();

const totals = computed(() => {
  const files = props.git?.files ?? [];
  return {
    count: files.length,
    additions: files.reduce((n, f) => n + f.additions, 0),
    deletions: files.reduce((n, f) => n + f.deletions, 0),
  };
});
</script>

<template>
  <footer class="statusbar">
    <span class="cwd" :title="cwd ?? ''">
      <svg viewBox="0 0 16 16" width="13" height="13"><path d="M2 4.5A1.5 1.5 0 0 1 3.5 3h3l1.5 1.5h4.5A1.5 1.5 0 0 1 14 6v5.5a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 11.5z" fill="none" stroke="currentColor" stroke-width="1.2" /></svg>
      {{ cwd ? prettyPath(cwd, home) : "" }}
    </span>

    <div class="right" @mousedown.prevent>
      <button class="btn" :class="{ open: panelOpen }" title="Terminal inferior (Ctrl+')" @click="$emit('togglePanel')">
        <svg viewBox="0 0 16 16" width="13" height="13"><rect x="2" y="2.5" width="12" height="11" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.2" /><path d="M2 9.5h12" stroke="currentColor" stroke-width="1.2" /></svg>
        Terminal
      </button>

      <template v-if="git">
        <button
          class="btn"
          :class="{ open: pickerOpen, dirty: totals.count > 0 }"
          :title="`${git.root}\nTrocar de branch`"
          @click="$emit('pickBranch')"
        >
          <svg viewBox="0 0 16 16" width="13" height="13"><circle cx="5" cy="3.5" r="1.5" fill="none" stroke="currentColor" stroke-width="1.2" /><circle cx="5" cy="12.5" r="1.5" fill="none" stroke="currentColor" stroke-width="1.2" /><circle cx="11" cy="6" r="1.5" fill="none" stroke="currentColor" stroke-width="1.2" /><path d="M5 5v6M11 7.5c0 2-2 2.5-5 3.5" fill="none" stroke="currentColor" stroke-width="1.2" /></svg>
          <span class="repo">{{ repoName(git.root) }}</span>
          <span class="slash">/</span>
          <span class="branch">{{ git.branch ?? "HEAD" }}</span>
          <span v-if="git.ahead" class="sync">↑{{ git.ahead }}</span>
          <span v-if="git.behind" class="sync">↓{{ git.behind }}</span>
          <svg viewBox="0 0 16 16" width="10" height="10"><path d="M4 6l4 4 4-4" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" /></svg>
        </button>

        <button class="btn" :class="{ open: diffOpen }" title="Explorador de arquivos (Ctrl+Shift+G)" @click="$emit('toggleDiff')">
          <template v-if="totals.count">
            <span>{{ totals.count }} {{ totals.count === 1 ? "arquivo" : "arquivos" }}</span>
            <span class="add">+{{ totals.additions }}</span>
            <span class="del">−{{ totals.deletions }}</span>
          </template>
          <span v-else class="clean">sem alterações</span>
        </button>
      </template>
    </div>
  </footer>
</template>

<style scoped>
.statusbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  height: 26px;
  padding: 0 10px;
  background: var(--bg-elevated);
  border-top: 1px solid var(--border);
  color: var(--fg-muted);
  font-size: 12px;
  user-select: none;
}
.cwd {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  overflow: hidden;
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
.repo {
  color: var(--fg);
  font-weight: 500;
}
.slash {
  opacity: 0.5;
}
.btn.dirty .branch {
  color: var(--yellow);
}
.add {
  color: var(--green);
}
.del {
  color: var(--red);
}
.clean,
.sync {
  opacity: 0.8;
}
</style>
