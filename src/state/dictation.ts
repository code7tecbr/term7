import { shallowRef } from "vue";

/** Para onde vai o ditado em vez do terminal com o foco (o campo de resposta da visão geral). */
export interface DictationSink {
  tabId: number;
  put: (text: string, send: boolean) => void;
}

export const dictationSink = shallowRef<DictationSink | null>(null);
