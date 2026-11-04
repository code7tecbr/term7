import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const { reviewDecision } = await import("./github");

describe("reviewDecision", () => {
  it("usa o do GitHub quando a branch exige aprovação", () => {
    expect(reviewDecision({ reviewDecision: "REVIEW_REQUIRED", latestReviews: [{ state: "APPROVED" }] })).toBe("REVIEW_REQUIRED");
  });

  it("sem a regra, vem dos últimos reviews (alteração pedida ganha)", () => {
    expect(reviewDecision({ reviewDecision: "", latestReviews: [{ state: "APPROVED" }, { state: "CHANGES_REQUESTED" }] })).toBe(
      "CHANGES_REQUESTED",
    );
    expect(reviewDecision({ reviewDecision: "", latestReviews: [{ state: "COMMENTED" }, { state: "APPROVED" }] })).toBe("APPROVED");
  });

  it("só comentário ou nenhum review: sem decisão", () => {
    expect(reviewDecision({ reviewDecision: "", latestReviews: [{ state: "COMMENTED" }] })).toBe(null);
    expect(reviewDecision({})).toBe(null);
  });
});
