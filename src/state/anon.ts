import { ref, watch } from "vue";

/**
 * Modo anônimo, pra gravar a tela: tudo passa a usar uma fonte em que as letras
 * e números desenham outros caracteres (scripts/anon-font.py). Parece texto,
 * mas não dá pra ler; o conteúdo em si não muda, então desligar volta na hora.
 */
export const anon = ref(false);

export const ANON_FONT = '"term7 Anon"';

/** O xterm desenha num canvas: a fonte vai pela opção dele, não pelo CSS. */
export async function anonFontReady() {
  try {
    await document.fonts.load(`14px ${ANON_FONT}`);
  } catch {
    // sem a fonte, o xterm segue com a reserva
  }
}

// Dica do sistema (atributo title) não usa a fonte da página: no modo anônimo
// ela é guardada à parte e volta quando o modo desliga.
function hideTitle(e: MouseEvent) {
  const el = (e.target as HTMLElement | null)?.closest?.("[title]") as HTMLElement | null;
  if (!el) return;
  el.dataset.anonTitle = el.title;
  el.removeAttribute("title");
}
watch(anon, (on) => {
  if (on) {
    document.addEventListener("mouseover", hideTitle, true);
    return;
  }
  document.removeEventListener("mouseover", hideTitle, true);
  document.querySelectorAll<HTMLElement>("[data-anon-title]").forEach((el) => {
    el.title = el.dataset.anonTitle ?? "";
    delete el.dataset.anonTitle;
  });
});
