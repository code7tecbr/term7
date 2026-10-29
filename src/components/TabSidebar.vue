<script setup lang="ts">
import { computed, nextTick, ref } from "vue";
import ContextMenu, { type MenuItem } from "./ContextMenu.vue";
import {
  COLORS,
  TAGS,
  findTag,
  setTabTag,
  setGroupTag,
  type TagId,
  activate,
  closeGroup,
  closeOtherTabs,
  closeTab,
  createGroup,
  findGroup,
  moveGroupTo,
  moveTabTo,
  newTab,
  newTabInGroup,
  prettyPath,
  renameGroup,
  renameTab,
  setGroupColor,
  setTabColor,
  setTabGroup,
  store,
  tabTitle,
  toggleCollapsed,
  ungroup,
  type DropTarget,
  type Group,
  type Tab,
} from "../state/tabs";

const props = defineProps<{ home: string | null }>();
// Tem mais de um elemento raiz (lista, menu, fantasma do arrasto): o style vai pro <aside>.
defineOptions({ inheritAttrs: false });

// Trechos contíguos da lista: abas soltas ou um grupo com as abas dele.
type Section = { group: Group | null; tabs: Tab[] };

const sections = computed<Section[]>(() => {
  const out: Section[] = [];
  for (const tab of store.tabs) {
    const group = findGroup(tab.groupId) ?? null;
    const last = out[out.length - 1];
    if (last && last.group?.id === group?.id) last.tabs.push(tab);
    else out.push({ group, tabs: [tab] });
  }
  return out;
});

/** Faixa da aba ativa: na cor dela, senão na do grupo, senão a de destaque. */
const stripe = (tab: Tab) => tab.color ?? findGroup(tab.groupId)?.color ?? "var(--accent)";
const isClaude = (tab: Tab) => tab.process === "claude";

/** Grupo recolhido herda o aviso mais forte das abas dele, pra não esconder um pedido. */
function groupAttention(tabs: Tab[]) {
  if (tabs.some((t) => t.attention === "waiting")) return "attn-waiting";
  if (tabs.some((t) => t.attention === "done")) return "attn-done";
  return "";
}

const ATTENTION_TITLE = { waiting: "O Claude está esperando você", done: "O Claude terminou" } as const;

// ---- renomear (aba ou grupo) ----
const editing = ref<{ kind: "tab" | "group"; id: number } | null>(null);
const editName = ref("");
const editInput = ref<HTMLInputElement[]>();

async function startRename(kind: "tab" | "group", id: number, current: string) {
  editing.value = { kind, id };
  editName.value = current;
  await nextTick();
  editInput.value?.[0]?.select();
}

function commitRename() {
  const e = editing.value;
  if (!e) return;
  if (e.kind === "tab") renameTab(e.id, editName.value);
  else renameGroup(e.id, editName.value);
  editing.value = null;
}

const isEditing = (kind: "tab" | "group", id: number) => editing.value?.kind === kind && editing.value.id === id;

// ---- arrastar e soltar ----
type DragSource = { kind: "tab" | "group"; id: number; label: string };

const list = ref<HTMLElement>();
const dragging = ref<DragSource | null>(null);
const hint = ref<DropTarget | null>(null);
const ghost = ref({ x: 0, y: 0 });
let pending: { src: DragSource; x: number; y: number } | null = null;
// O clique que vem logo depois de soltar não pode ativar/recolher nada.
let suppressClick = false;

function startDrag(e: PointerEvent, src: DragSource) {
  if (e.button !== 0 || editing.value) return;
  pending = { src, x: e.clientX, y: e.clientY };
  window.addEventListener("pointermove", onDragMove);
  window.addEventListener("pointerup", onDragEnd, { once: true });
}

function onDragMove(e: PointerEvent) {
  if (!dragging.value) {
    // Só vira arrasto depois de mover alguns pixels; antes disso é clique.
    if (!pending || Math.hypot(e.clientX - pending.x, e.clientY - pending.y) < 5) return;
    dragging.value = pending.src;
  }
  ghost.value = { x: e.clientX, y: e.clientY };
  hint.value = findTarget(e.clientX, e.clientY);
  autoScroll(e.clientY);
}

function onDragEnd() {
  window.removeEventListener("pointermove", onDragMove);
  const src = dragging.value;
  if (src && hint.value) {
    if (src.kind === "tab") moveTabTo(src.id, hint.value);
    else moveGroupTo(src.id, hint.value);
  }
  if (src) {
    suppressClick = true;
    setTimeout(() => (suppressClick = false));
  }
  dragging.value = null;
  hint.value = null;
  pending = null;
}

function findTarget(x: number, y: number): DropTarget | null {
  const el = document.elementFromPoint(x, y) as HTMLElement | null;
  if (!el || !list.value?.contains(el)) return null;
  const src = dragging.value!;

  const tabEl = el.closest<HTMLElement>("[data-tab-id]");
  if (tabEl) {
    const tab = store.tabs.find((t) => t.id === Number(tabEl.dataset.tabId));
    if (!tab) return null;
    const rect = tabEl.getBoundingClientRect();
    const pos = y < rect.top + rect.height / 2 ? "before" : "after";
    if (src.kind === "tab") return tab.id === src.id ? null : { kind: "tab", id: tab.id, pos };
    // Grupo arrastado sobre aba de outro grupo: cai antes/depois do grupo inteiro.
    if (tab.groupId === src.id) return null;
    return tab.groupId !== null ? { kind: "group", id: tab.groupId, pos } : { kind: "tab", id: tab.id, pos };
  }

  const groupEl = el.closest<HTMLElement>("[data-group-id]");
  if (groupEl) {
    const id = Number(groupEl.dataset.groupId);
    const rect = groupEl.getBoundingClientRect();
    if (src.kind === "group") {
      if (id === src.id) return null;
      return { kind: "group", id, pos: y < rect.top + rect.height / 2 ? "before" : "after" };
    }
    return { kind: "group", id, pos: y < rect.top + rect.height * 0.35 ? "before" : "inside" };
  }

  if (el.closest(".end-zone")) return { kind: "end" };
  // No vão entre as linhas: mantém o último alvo pra não piscar.
  return hint.value;
}

function autoScroll(y: number) {
  const el = list.value;
  if (!el) return;
  const rect = el.getBoundingClientRect();
  if (y < rect.top + 32) el.scrollTop -= 10;
  else if (y > rect.bottom - 32) el.scrollTop += 10;
}

const dropClass = (kind: "tab" | "group", id: number) => {
  const h = hint.value;
  if (!h || h.kind !== kind || h.id !== id) return "";
  return `drop-${h.pos}`;
};

function onTabClick(tab: Tab) {
  if (!suppressClick) activate(tab.id);
}

function onGroupClick(group: Group) {
  if (!suppressClick && !isEditing("group", group.id)) toggleCollapsed(group.id);
}

// ---- menus de contexto ----
const menu = ref<{ x: number; y: number; items: MenuItem[] } | null>(null);

function tabMenu(e: MouseEvent, tab: Tab) {
  const others = store.groups.filter((g) => g.id !== tab.groupId);
  menu.value = {
    x: e.clientX,
    y: e.clientY,
    items: [
      { colors: COLORS, selected: tab.color, allowNone: true, pick: (c) => setTabColor(tab.id, c) },
      { tags: TAGS, selected: tab.tag, pick: (t) => setTabTag(tab.id, t as TagId | null) },
      { label: "Renomear aba", action: () => startRename("tab", tab.id, tabTitle(tab, props.home)) },
      ...(tab.name ? [{ label: "Usar nome automático", action: () => renameTab(tab.id, null) }] : []),
      { separator: true },
      {
        label: "Adicionar a novo grupo",
        action: () => {
          const g = createGroup(tab.id);
          startRename("group", g.id, g.name);
        },
      },
      ...others.map((g) => ({
        label: `Mover para “${g.name}”`,
        dot: g.color,
        action: () => setTabGroup(tab.id, g.id),
      })),
      ...(tab.groupId !== null ? [{ label: "Remover do grupo", action: () => setTabGroup(tab.id, null) }] : []),
      { separator: true },
      { label: "Fechar outras abas", action: () => closeOtherTabs(tab.id) },
      { label: "Fechar aba", action: () => closeTab(tab.id), danger: true },
    ],
  };
}

function groupMenu(e: MouseEvent, group: Group) {
  menu.value = {
    x: e.clientX,
    y: e.clientY,
    items: [
      { colors: COLORS, selected: group.color, pick: (c) => c && setGroupColor(group.id, c) },
      { tags: TAGS, selected: group.tag, pick: (t) => setGroupTag(group.id, t as TagId | null) },
      { label: "Renomear grupo", action: () => startRename("group", group.id, group.name) },
      { label: "Nova aba no grupo", action: () => newTabInGroup(group.id) },
      { label: group.collapsed ? "Expandir" : "Recolher", action: () => toggleCollapsed(group.id) },
      { label: "Desagrupar", action: () => ungroup(group.id) },
      { separator: true },
      { label: "Fechar grupo", action: () => closeGroup(group.id), danger: true },
    ],
  };
}
</script>

<template>
  <aside class="sidebar" v-bind="$attrs">
    <header>
      <span>Abas</span>
      <button class="icon" title="Nova aba (Ctrl+Shift+T)" @mousedown.prevent @click="newTab()">
        <svg viewBox="0 0 16 16" width="14" height="14"><path d="M8 3v10M3 8h10" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" /></svg>
      </button>
    </header>

    <div ref="list" class="list" :class="{ dragging: dragging }" @dblclick.self="newTab()">
      <section
        v-for="section in sections"
        :key="section.group ? `g${section.group.id}` : `t${section.tabs[0].id}`"
        :class="[
          { group: section.group },
          section.group && hint?.kind === 'group' && hint.id === section.group.id && hint.pos !== 'inside'
            ? `drop-${hint.pos}`
            : '',
        ]"
        :style="section.group ? { '--group': section.group.color } : undefined"
      >
        <div
          v-if="section.group"
          class="group-header"
          :class="{
            'drop-inside': hint?.kind === 'group' && hint.id === section.group.id && hint.pos === 'inside',
            lifted: dragging?.kind === 'group' && dragging.id === section.group.id,
            [groupAttention(section.tabs)]: section.group.collapsed,
          }"
          :data-group-id="section.group.id"
          @pointerdown="startDrag($event, { kind: 'group', id: section.group.id, label: section.group.name })"
          @mousedown="!isEditing('group', section.group.id) && $event.preventDefault()"
          @click="onGroupClick(section.group)"
          @dblclick.stop="startRename('group', section.group.id, section.group.name)"
          @contextmenu.prevent="groupMenu($event, section.group)"
        >
          <svg class="chevron" :class="{ open: !section.group.collapsed }" viewBox="0 0 16 16" width="12" height="12"><path d="M6 4l4 4-4 4" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" /></svg>
          <span class="dot" />
          <input
            v-if="isEditing('group', section.group.id)"
            ref="editInput"
            v-model="editName"
            class="rename"
            @keydown.enter="commitRename"
            @keydown.esc="editing = null"
            @blur="commitRename"
            @click.stop
          />
          <span v-else class="name">{{ section.group.name }}</span>
          <span v-if="findTag(section.group.tag)" class="tag" :style="{ '--tag': findTag(section.group.tag)!.color }">{{ findTag(section.group.tag)!.label }}</span>
          <span v-if="section.group.collapsed && groupAttention(section.tabs)" class="attn-dot" />
          <span class="count">{{ section.tabs.length }}</span>
        </div>

        <template v-if="!section.group?.collapsed">
          <div
            v-for="tab in section.tabs"
            :key="tab.id"
            class="tab"
            :class="[
              {
                active: tab.id === store.activeId,
                colored: !!tab.color,
                'attn-waiting': tab.attention === 'waiting',
                'attn-done': tab.attention === 'done',
                lifted: (dragging?.kind === 'tab' && dragging.id === tab.id) || (dragging?.kind === 'group' && dragging.id === tab.groupId),
              },
              dropClass('tab', tab.id),
            ]"
            :data-tab-id="tab.id"
            :style="{ '--stripe': stripe(tab), '--fill': tab.color ?? undefined }"
            :title="tab.cwd ?? ''"
            @mousedown="!isEditing('tab', tab.id) && ($event.preventDefault(), $event.button === 1 && closeTab(tab.id))"
            @pointerdown="startDrag($event, { kind: 'tab', id: tab.id, label: tabTitle(tab, props.home) })"
            @click="onTabClick(tab)"
            @dblclick="startRename('tab', tab.id, tabTitle(tab, props.home))"
            @contextmenu.prevent="tabMenu($event, tab)"
          >
            <span
              class="icon-slot"
              :class="[{ claude: isClaude(tab) }, tab.claude && isClaude(tab) ? `claude-${tab.claude}` : '']"
              :title="tab.attention ? ATTENTION_TITLE[tab.attention] : ''"
            >
              <svg v-if="isClaude(tab)" viewBox="0 0 16 16" width="14" height="14"><path d="M8 1.5v13M1.5 8h13M3.4 3.4l9.2 9.2M12.6 3.4l-9.2 9.2" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" /></svg>
              <svg v-else viewBox="0 0 16 16" width="14" height="14"><path d="M3 4.5l3.5 3.5L3 11.5M8.5 12h4.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" /></svg>
            </span>
            <div class="text">
              <input
                v-if="isEditing('tab', tab.id)"
                ref="editInput"
                v-model="editName"
                class="rename"
                @keydown.enter="commitRename"
                @keydown.esc="editing = null"
                @blur="commitRename"
                @click.stop
                @dblclick.stop
              />
              <span v-else class="title-row">
                <span class="title">{{ tabTitle(tab, props.home) }}</span>
                <span v-if="findTag(tab.tag)" class="tag" :style="{ '--tag': findTag(tab.tag)!.color }">{{ findTag(tab.tag)!.label }}</span>
              </span>
              <span v-if="tab.cwd" class="sub">{{ prettyPath(tab.cwd, props.home) }}</span>
            </div>
            <span v-if="tab.attention === 'done'" class="attn-dot" title="O Claude terminou" />
            <button class="close" title="Fechar (Ctrl+Shift+W)" @pointerdown.stop @click.stop="closeTab(tab.id)" @dblclick.stop>
              <svg viewBox="0 0 16 16" width="12" height="12"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" /></svg>
            </button>
          </div>
        </template>
      </section>
      <div class="end-zone" :class="{ 'drop-end': hint?.kind === 'end' }" @dblclick="newTab()" />
    </div>
  </aside>

  <div v-if="dragging" class="ghost" :style="{ left: ghost.x + 12 + 'px', top: ghost.y + 8 + 'px' }">
    {{ dragging.kind === "group" ? "Grupo: " : "" }}{{ dragging.label }}
  </div>

  <ContextMenu v-if="menu" v-bind="menu" @close="menu = null" />
</template>

<style scoped>
.sidebar {
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  height: 100%;
  min-width: 0;
  background: var(--bg-elevated);
  user-select: none;
  font-size: 12.5px;
}
header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 36px;
  padding: 0 6px 0 14px;
  flex-shrink: 0;
  color: var(--fg-muted);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
}
.icon {
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
.icon:hover {
  background: var(--hover);
  color: var(--fg);
}
.list {
  display: flex;
  flex-direction: column;
  flex: 1;
  padding: 0 6px 8px;
  overflow-y: auto;
}
.list.dragging {
  cursor: grabbing;
}
section {
  position: relative;
}
.end-zone {
  position: relative;
  flex: 1;
  min-height: 48px;
}
/* Linha azul que mostra onde o item vai cair. */
section.drop-before::before,
section.drop-after::after,
.tab.drop-before::after,
.tab.drop-after::after,
.end-zone.drop-end::before {
  content: "";
  position: absolute;
  left: 6px;
  right: 6px;
  height: 2px;
  border-radius: 1px;
  background: var(--accent);
  z-index: 2;
}
section.drop-before::before,
.tab.drop-before::after,
.end-zone.drop-end::before {
  top: -2px;
}
section.drop-after::after,
.tab.drop-after::after {
  bottom: -2px;
}
.group-header.drop-inside {
  background: color-mix(in srgb, var(--group) 22%, transparent);
  box-shadow: inset 0 0 0 1px var(--group);
}
.lifted {
  opacity: 0.4;
}
.ghost {
  position: fixed;
  z-index: 200;
  max-width: 220px;
  padding: 4px 10px;
  overflow: hidden;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--bg-elevated);
  box-shadow: 0 6px 18px rgb(0 0 0 / 0.4);
  color: var(--fg);
  font-size: 12px;
  white-space: nowrap;
  text-overflow: ellipsis;
  pointer-events: none;
}
/* O grupo inteiro fica com um fundo leve na cor dele; o cabeçalho, mais forte. */
section.group {
  margin: 4px 0;
  padding-bottom: 2px;
  border-radius: 7px;
  background: color-mix(in srgb, var(--group) 7%, transparent);
}
.group-header {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 26px;
  padding: 0 8px 0 4px;
  border-radius: 6px;
  color: var(--fg);
  font-weight: 600;
  cursor: pointer;
}
.group-header {
  background: color-mix(in srgb, var(--group) 20%, transparent);
}
.group-header:hover {
  background: color-mix(in srgb, var(--group) 27%, transparent);
}
.chevron {
  flex-shrink: 0;
  color: var(--fg-muted);
  transition: transform 0.12s;
}
.chevron.open {
  transform: rotate(90deg);
}
.dot {
  width: 8px;
  height: 8px;
  flex-shrink: 0;
  border-radius: 50%;
  background: var(--group);
}
.name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}
.count {
  color: var(--fg-muted);
  font-size: 11px;
  font-weight: 500;
}
/* Abas do grupo ficam recuadas; a faixa colorida marca só a aba ativa. */
section.group .tab {
  margin: 1px 4px 1px 10px;
}
section.group > .tab:first-of-type {
  margin-top: 2px;
}
.tab {
  position: relative;
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 38px;
  margin: 1px 0;
  padding: 4px 4px 4px 12px;
  border-radius: 6px;
  color: var(--fg-muted);
  cursor: pointer;
}
.tab::before {
  content: "";
  position: absolute;
  left: 3px;
  top: 8px;
  bottom: 8px;
  width: 3px;
  border-radius: 2px;
}
.tab.active::before {
  top: 6px;
  bottom: 6px;
  background: var(--stripe);
}
.tab:hover {
  background: var(--hover);
}
.tab.active {
  background: var(--active);
  color: var(--fg);
}
/* Cor da aba preenche o fundo, mais forte quando ativa. */
.tab.colored {
  background: color-mix(in srgb, var(--fill) 14%, transparent);
  color: var(--fg);
}
.tab.colored:hover {
  background: color-mix(in srgb, var(--fill) 20%, transparent);
}
.tab.colored.active {
  background: color-mix(in srgb, var(--fill) 30%, transparent);
}
.icon-slot {
  display: grid;
  place-items: center;
  flex-shrink: 0;
  width: 16px;
  opacity: 0.8;
}
.icon-slot.claude {
  color: var(--claude);
  opacity: 1;
}
/* Avisos do Claude (hooks): trabalhando gira o ícone; esperando você chama atenção. */
.icon-slot.claude-working svg {
  animation: spin 2.4s linear infinite;
}
.icon-slot.claude-waiting {
  animation: blink 1.2s ease-in-out infinite;
}
.tab.attn-waiting,
.group-header.attn-waiting {
  background: color-mix(in srgb, var(--claude) 18%, transparent);
  color: var(--fg);
  animation: attn 1.6s ease-in-out infinite;
}
.attn-dot {
  flex-shrink: 0;
  width: 7px;
  height: 7px;
  margin-right: 4px;
  border-radius: 50%;
  background: var(--green);
}
.group-header.attn-waiting .attn-dot {
  background: var(--claude);
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
@keyframes blink {
  50% {
    opacity: 0.35;
  }
}
@keyframes attn {
  0%,
  100% {
    box-shadow: inset 0 0 0 1.5px var(--claude);
  }
  50% {
    box-shadow: inset 0 0 0 1.5px color-mix(in srgb, var(--claude) 35%, transparent);
  }
}
.text {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-width: 0;
}
.title,
.sub {
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}
.title {
  font-weight: 500;
}
.title-row {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}
.tag {
  flex-shrink: 0;
  padding: 0 5px;
  border-radius: 4px;
  background: color-mix(in srgb, var(--tag) 18%, transparent);
  color: var(--tag);
  font-size: 10px;
  font-weight: 600;
  line-height: 15px;
  letter-spacing: 0.02em;
}
.sub {
  color: var(--fg-muted);
  font-size: 11px;
  opacity: 0.75;
}
.rename {
  width: 100%;
  min-width: 0;
  padding: 1px 4px;
  border: 1px solid var(--accent);
  border-radius: 4px;
  outline: 0;
  background: var(--bg);
  color: var(--fg);
  font: inherit;
}
.close {
  display: grid;
  place-items: center;
  flex-shrink: 0;
  width: 20px;
  height: 20px;
  border: 0;
  border-radius: 4px;
  background: none;
  color: inherit;
  opacity: 0;
  cursor: pointer;
}
.tab:hover .close {
  opacity: 0.7;
}
.close:hover {
  background: var(--hover);
  opacity: 1 !important;
}
</style>
