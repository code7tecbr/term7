import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const { importAt } = await import("./goto");

describe("importAt", () => {
  const line = 'import FileView from "../components/FileView.vue";';

  it("acha o import relativo sob o cursor", () => {
    expect(importAt(line, line.indexOf("FileView.vue"))).toBe("../components/FileView.vue");
    expect(importAt("const x = require('./x');", 20)).toBe("./x");
  });

  it("ignora o resto da linha e pacotes", () => {
    expect(importAt(line, 8)).toBeNull();
    expect(importAt('import { ref } from "vue";', 22)).toBeNull();
  });
});
