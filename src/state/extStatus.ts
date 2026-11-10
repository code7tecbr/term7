import { reactive } from "vue";

// Status publicado por extensões (ex.: statusbot) pela API local: a extensão
// manda os itens e o term7 só desenha, na barra de status.

export type StatusLevel = "up" | "degraded" | "maintenance" | "down" | "unknown";
const LEVELS: StatusLevel[] = ["down", "degraded", "unknown", "maintenance", "up"];

export const LEVEL_LABEL: Record<StatusLevel, string> = {
  up: "normal",
  degraded: "degradado",
  maintenance: "em manutenção",
  down: "fora do ar",
  unknown: "sem leitura",
};

export interface StatusItem {
  label: string;
  status: StatusLevel;
  /** ISO: desde quando está nesse estado. */
  since?: string;
  detail?: string;
  link?: string;
}

export interface StatusSource {
  id: string;
  title: string;
  items: StatusItem[];
  /** Problema da própria extensão (ex.: não alcança o serviço). */
  error: string | null;
  /** ISO: quando o serviço leu pela última vez. */
  checkedAt: string | null;
  link: string | null;
  /** Quando o term7 recebeu (ms) e quanto vale sem nova atualização. */
  receivedAt: number;
  ttlMs: number;
}

export const extStatus = reactive<{ sources: Record<string, StatusSource> }>({ sources: {} });

const MAX_ITEMS = 200;
const str = (v: unknown, max: number) => (typeof v === "string" && v.trim() ? v.trim().slice(0, max) : undefined);
/** Link só http(s): é aberto no navegador. */
const httpLink = (v: unknown) => {
  const s = str(v, 2000);
  return s && /^https?:\/\//i.test(s) ? s : undefined;
};
const isoDate = (v: unknown) => {
  const s = str(v, 40);
  return s && !Number.isNaN(Date.parse(s)) ? s : undefined;
};

/** Valida o `status.set` de uma extensão. Lança Error com a mensagem pra extensão. */
export function parseStatusSet(params: Record<string, unknown>, now: number): StatusSource {
  const id = str(params.source, 40);
  if (!id || !/^[\w.-]+$/.test(id)) throw new Error("\"source\" precisa ser um id curto (letras, números, _ . -)");
  if (!Array.isArray(params.items)) throw new Error("\"items\" precisa ser uma lista");
  if (params.items.length > MAX_ITEMS) throw new Error(`no máximo ${MAX_ITEMS} itens`);
  const items = params.items.map((raw, i): StatusItem => {
    const it = (raw ?? {}) as Record<string, unknown>;
    const label = str(it.label, 120);
    if (!label) throw new Error(`item ${i}: falta "label"`);
    const status = LEVELS.includes(it.status as StatusLevel) ? (it.status as StatusLevel) : "unknown";
    return { label, status, since: isoDate(it.since), detail: str(it.detail, 300), link: httpLink(it.link) };
  });
  const ttl = Number(params.ttlSeconds);
  return {
    id,
    title: str(params.title, 40) ?? id,
    items,
    error: str(params.error, 300) ?? null,
    checkedAt: isoDate(params.checkedAt) ?? null,
    link: httpLink(params.link) ?? null,
    receivedAt: now,
    // Entre 30 s e 1 dia; padrão 5 min.
    ttlMs: (Number.isFinite(ttl) ? Math.min(Math.max(ttl, 30), 86_400) : 300) * 1000,
  };
}

/** A extensão parou de mandar: o que está na tela pode estar velho. */
export const isStale = (s: StatusSource, now: number) => now - s.receivedAt > s.ttlMs;

/** Pior estado da fonte, contando erro e dado velho como "sem leitura". */
export function worst(s: StatusSource, now: number): StatusLevel {
  const levels = s.items.map((i) => i.status);
  if (s.error || isStale(s, now)) levels.push("unknown");
  return LEVELS.find((l) => levels.includes(l)) ?? "up";
}

/** Itens fora do normal primeiro (a ordem da extensão se mantém dentro de cada nível). */
export const sorted = (items: StatusItem[]) =>
  [...items].sort((a, b) => LEVELS.indexOf(a.status) - LEVELS.indexOf(b.status));

/** "agora", "há 5 min", "há 2 h", "há 3 d". */
export function ago(iso: string | null | undefined, now: number) {
  if (!iso) return "";
  const min = Math.floor((now - Date.parse(iso)) / 60_000);
  if (min < 1) return "agora";
  if (min < 60) return `há ${min} min`;
  if (min < 48 * 60) return `há ${Math.floor(min / 60)} h`;
  return `há ${Math.floor(min / 1440)} d`;
}
