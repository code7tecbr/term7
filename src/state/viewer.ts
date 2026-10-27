import { reactive } from "vue";

export interface OpenFile {
  root: string;
  /** Caminho relativo à raiz do repositório. */
  path: string;
  /** Nome antigo, quando o arquivo foi renomeado (pra comparar com o HEAD). */
  origPath: string | null;
}

/** Arquivos abertos no visualizador (ao lado do terminal). */
export const viewer = reactive({
  files: [] as OpenFile[],
  active: -1,
  mode: "changes" as "changes" | "file",
});

const same = (a: OpenFile, b: OpenFile) => a.root === b.root && a.path === b.path;

export function openFile(file: OpenFile) {
  const i = viewer.files.findIndex((f) => same(f, file));
  if (i >= 0) {
    viewer.active = i;
  } else {
    viewer.files.push(file);
    viewer.active = viewer.files.length - 1;
  }
}

export function closeFile(index: number) {
  viewer.files.splice(index, 1);
  if (viewer.active >= viewer.files.length) viewer.active = viewer.files.length - 1;
}

export function closeAllFiles() {
  viewer.files = [];
  viewer.active = -1;
}
