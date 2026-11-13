import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const { configToSend, formValues } = await import("./extensions");

const ext = {
  id: "statusbot",
  name: "statusbot",
  description: "",
  link: null,
  kind: "service" as const,
  config: [
    { key: "URL", label: "URL", type: "url" as const, required: true, default: null, placeholder: null, help: null },
    { key: "TOKEN", label: "Token", type: "secret" as const, required: false, default: null, placeholder: null, help: null },
    { key: "EVERY", label: "Intervalo", type: "number" as const, required: false, default: "60", placeholder: null, help: null },
  ],
  values: { URL: "http://localhost:8080" },
  secrets_set: ["TOKEN"],
  installed: true,
  running: true,
  accounts: [],
};

describe("formulário da extensão", () => {
  it("começa com o salvo ou o padrão, e o secreto vazio", () => {
    expect(formValues(ext)).toEqual({ URL: "http://localhost:8080", TOKEN: "", EVERY: "60" });
  });

  it("secreto vazio mantém o salvo, a não ser que peçam pra apagar", () => {
    const form = { URL: "http://x", TOKEN: "", EVERY: "30" };
    expect(configToSend(ext, form)).toEqual({ URL: "http://x", TOKEN: null, EVERY: "30" });
    expect(configToSend(ext, form, new Set(["TOKEN"]))).toEqual({ URL: "http://x", TOKEN: "", EVERY: "30" });
    expect(configToSend(ext, { ...form, TOKEN: "novo" }).TOKEN).toBe("novo");
    // Campo numérico editado na tela chega como número.
    expect(configToSend(ext, { ...form, EVERY: 45 }).EVERY).toBe("45");
  });
});
