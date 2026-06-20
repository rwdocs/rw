import { describe, it, expect } from "vitest";
import { documentIdFor } from "./documentId";
import type { PageMeta } from "../../types";

function makeMeta(overrides: Partial<PageMeta> = {}): PageMeta {
  return {
    title: "Test",
    path: "/billing/overview",
    sourceFile: "billing/overview.md",
    lastModified: "2026-06-20T00:00:00Z",
    sectionRef: "domain:default/billing",
    subpath: "overview",
    ...overrides,
  };
}

describe("documentIdFor", () => {
  it("joins sectionRef and subpath with '#'", () => {
    expect(documentIdFor(makeMeta())).toBe("domain:default/billing#overview");
  });

  it("handles a section-root page (empty subpath)", () => {
    const id = documentIdFor(makeMeta({ subpath: "", path: "/billing" }));
    expect(id).toBe("domain:default/billing#");
  });

  it("ignores the URL path entirely (stable across relocation)", () => {
    const a = documentIdFor(makeMeta({ path: "/billing/overview" }));
    const b = documentIdFor(makeMeta({ path: "/moved/billing/overview" }));
    expect(a).toBe(b);
  });

  it("handles a deep subpath", () => {
    const id = documentIdFor(
      makeMeta({ sectionRef: "system:default/payments", subpath: "api/auth" }),
    );
    expect(id).toBe("system:default/payments#api/auth");
  });
});
