<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import { spawnPty, type PtySession } from "../lib/pty";
import { attachLinks } from "../lib/links";
import { matchShortcut } from "../lib/shortcuts";
import { isTextKey, routeTextInput } from "../lib/textInput";
import { term7Dark } from "../lib/theme";

const props = defineProps<{ cwd: string | null; focused: boolean }>();
const emit = defineEmits<{ spawned: [session: PtySession]; exit: [code: number]; focus: [] }>();

const FONT_SIZE = 14;

const container = ref<HTMLDivElement>();
let term: Terminal | undefined;
let session: PtySession | undefined;
let observer: ResizeObserver | undefined;
const fit = new FitAddon();

function loadWebgl(t: Terminal) {
  try {
    const webgl = new WebglAddon();
    // Se a GPU perder o contexto, volta pro renderizador padrão (DOM).
    webgl.onContextLoss(() => webgl.dispose());
    t.loadAddon(webgl);
  } catch (err) {
    console.warn("[term7] WebGL indisponível, usando renderizador DOM", err);
  }
}

// O xterm mede a largura das células na hora que abre. Se a fonte ainda não
// carregou, ele mede a fonte reserva e os ícones/alinhamento ficam quebrados.
async function loadFont(family: string) {
  try {
    await document.fonts.load(`${FONT_SIZE}px ${family}`);
  } catch {
    // sem a fonte, segue com a reserva
  }
}

/**
 * Shift+Enter manda ESC + CR, que o Claude CLI entende como nova linha (sem
 * precisar do /terminal-setup). No zsh vira quebra de linha no comando.
 * Retorna true se tratou a tecla.
 */
function handleShiftEnter(e: KeyboardEvent) {
  if (e.key !== "Enter" || !e.shiftKey || e.ctrlKey || e.altKey || e.metaKey) return false;
  // Bloqueia também o keypress, senão o xterm manda o \r depois.
  if (e.type === "keydown") session?.write("\x1b\r");
  return true;
}

/**
 * Ctrl+Shift+C / Ctrl+Shift+V, e Ctrl+C com texto selecionado (copia em vez de
 * mandar a interrupção, como no VS Code). Sem seleção, Ctrl+C segue pro programa.
 * Retorna true se tratou a tecla.
 */
function handleClipboard(e: KeyboardEvent, t: Terminal) {
  if (!e.ctrlKey || e.altKey || e.metaKey) return false;
  const key = e.key.toLowerCase();
  const copy = key === "c" && (e.shiftKey || t.hasSelection());
  if (copy) {
    if (e.type === "keydown") {
      const text = t.getSelection();
      if (text) navigator.clipboard.writeText(text).catch(() => {});
      // Ctrl+C sozinho limpa a seleção: o próximo Ctrl+C volta a interromper.
      if (!e.shiftKey) t.clearSelection();
    }
    return true;
  }
  if (key === "v" && e.shiftKey) {
    if (e.type === "keydown") {
      navigator.clipboard
        .readText()
        .then((text) => text && t.paste(text))
        .catch(() => {});
    }
    return true;
  }
  return false;
}

function focus() {
  nextTick(() => {
    fit.fit();
    term?.focus();
  });
}

watch(
  () => props.focused,
  (focused) => focused && focus(),
);

onMounted(async () => {
  const fontFamily = getComputedStyle(document.documentElement).getPropertyValue("--font-mono").trim();
  await loadFont(fontFamily);

  const t = new Terminal({
    theme: term7Dark,
    fontFamily,
    fontSize: FONT_SIZE,
    lineHeight: 1.2,
    cursorBlink: true,
    cursorStyle: "bar",
    allowProposedApi: true,
    scrollback: 10_000,
  });
  term = t;

  t.loadAddon(fit);
  t.open(container.value!);
  loadWebgl(t);
  attachLinks(t);
  fit.fit();

  // Atalhos do app não chegam no shell. Texto (incluindo acentos) não vai pelo
  // keydown: chega pelo routeTextInput, a partir do que o sistema digitou.
  t.attachCustomKeyEventHandler(
    (e) => !handleClipboard(e, t) && !handleShiftEnter(e) && matchShortcut(e) === null && !isTextKey(e),
  );
  routeTextInput(t, container.value!, (text) => session?.write(text));

  session = await spawnPty({
    cols: t.cols,
    rows: t.rows,
    cwd: props.cwd ?? undefined,
    onData: (bytes) => t.write(bytes),
    onExit: (code) => emit("exit", code),
  });
  emit("spawned", session);

  t.onData((input) => session?.write(input));
  t.onResize(({ cols, rows }) => session?.resize(cols, rows));

  observer = new ResizeObserver(() => fit.fit());
  observer.observe(container.value!);
  t.textarea?.addEventListener("focus", () => emit("focus"));
  if (props.focused) focus();
});

onBeforeUnmount(() => {
  observer?.disconnect();
  session?.kill();
  term?.dispose();
});
</script>

<template>
  <div ref="container" class="terminal" />
</template>

<style scoped>
.terminal {
  width: 100%;
  height: 100%;
  padding: 8px 4px 4px 12px;
}
</style>
