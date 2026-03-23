import { describe, it, expect, vi } from "vitest";
import {
  rewriteSectionRefLinks,
  sectionRefString,
  resolveNavTree,
  resolveBreadcrumbs,
} from "./sectionRefs";
import type { NavigationTree, Breadcrumb } from "../types";

function createContainer(html: string): HTMLElement {
  const el = document.createElement("div");
  el.innerHTML = html;
  return el;
}

describe("sectionRefString", () => {
  it("builds ref string from section info", () => {
    expect(sectionRefString({ kind: "domain", name: "billing" })).toBe("domain:default/billing");
  });
});

describe("rewriteSectionRefLinks", () => {
  it("rewrites links with resolved section refs", async () => {
    const container = createContainer(
      '<a href="/domains/billing/api" data-section-ref="domain:default/billing" data-section-path="api">Billing API</a>',
    );
    const resolver = vi.fn().mockResolvedValue({
      "domain:default/billing": "/catalog/default/domain/billing/docs",
    });

    await rewriteSectionRefLinks(container, resolver, () => "");

    const link = container.querySelector("a")!;
    expect(link.getAttribute("href")).toBe("/catalog/default/domain/billing/docs/api");
    expect(resolver).toHaveBeenCalledWith(["domain:default/billing"]);
  });

  it("falls back to basePath for unresolved refs", async () => {
    const container = createContainer(
      '<a href="/domains/billing" data-section-ref="domain:default/billing" data-section-path="">Billing</a>',
    );
    const resolver = vi.fn().mockResolvedValue({});

    await rewriteSectionRefLinks(
      container,
      resolver,
      () => "/catalog/default/system/my-service/docs",
    );

    const link = container.querySelector("a")!;
    expect(link.getAttribute("href")).toBe("/catalog/default/system/my-service/docs");
  });

  it("deduplicates refs before calling resolver", async () => {
    const container = createContainer(
      '<a href="/a" data-section-ref="domain:default/billing" data-section-path="a">A</a>' +
        '<a href="/b" data-section-ref="domain:default/billing" data-section-path="b">B</a>',
    );
    const resolver = vi.fn().mockResolvedValue({
      "domain:default/billing": "/catalog/default/domain/billing/docs",
    });

    await rewriteSectionRefLinks(container, resolver, () => "");

    expect(resolver).toHaveBeenCalledWith(["domain:default/billing"]);
    const links = container.querySelectorAll("a");
    expect(links[0].getAttribute("href")).toBe("/catalog/default/domain/billing/docs/a");
    expect(links[1].getAttribute("href")).toBe("/catalog/default/domain/billing/docs/b");
  });

  it("handles links with no section-path", async () => {
    const container = createContainer(
      '<a href="/domains/billing" data-section-ref="domain:default/billing">Billing</a>',
    );
    const resolver = vi.fn().mockResolvedValue({
      "domain:default/billing": "/catalog/default/domain/billing/docs",
    });

    await rewriteSectionRefLinks(container, resolver, () => "");

    const link = container.querySelector("a")!;
    expect(link.getAttribute("href")).toBe("/catalog/default/domain/billing/docs");
  });

  it("skips container with no section-ref links", async () => {
    const container = createContainer('<a href="/about">About</a>');
    const resolver = vi.fn();

    await rewriteSectionRefLinks(container, resolver, () => "");

    expect(resolver).not.toHaveBeenCalled();
  });
});

describe("resolveNavTree", () => {
  it("rewrites nav item paths with resolved refs", async () => {
    const tree: NavigationTree = {
      items: [
        {
          title: "Billing",
          path: "/domains/billing",
          section: { kind: "domain", name: "billing" },
          children: [
            { title: "API", path: "/domains/billing/api" },
            {
              title: "Payments",
              path: "/domains/billing/systems/payments",
              section: { kind: "system", name: "payments" },
            },
          ],
        },
        { title: "Guide", path: "/guide" },
      ],
    };
    const resolver = vi.fn().mockResolvedValue({
      "domain:default/billing": "/catalog/default/domain/billing/docs",
      "system:default/payments": "/catalog/default/system/payments/docs",
    });

    const result = await resolveNavTree(tree, resolver);

    expect(result.items[0].href).toBe("/catalog/default/domain/billing/docs");
    expect(result.items[0].path).toBe("/domains/billing"); // path preserved for keying/active state
    expect(result.items[0].children![0].href).toBeUndefined(); // no section, no href
    expect(result.items[0].children![0].path).toBe("/domains/billing/api");
    expect(result.items[0].children![1].href).toBe("/catalog/default/system/payments/docs");
    expect(result.items[1].href).toBeUndefined(); // no section, no href
    expect(result.items[1].path).toBe("/guide");
  });

  it("rewrites scope and parentScope paths", async () => {
    const tree: NavigationTree = {
      items: [],
      scope: {
        path: "/domains/billing",
        title: "Billing",
        section: { kind: "domain", name: "billing" },
      },
      parentScope: {
        path: "/",
        title: "Home",
        section: { kind: "root", name: "home" },
      },
    };
    const resolver = vi.fn().mockResolvedValue({
      "domain:default/billing": "/catalog/default/domain/billing/docs",
    });

    const result = await resolveNavTree(tree, resolver);

    expect(result.scope!.href).toBe("/catalog/default/domain/billing/docs");
    expect(result.scope!.path).toBe("/domains/billing"); // path preserved
    expect(result.parentScope!.href).toBeUndefined(); // unresolved, no href
    expect(result.parentScope!.path).toBe("/"); // path preserved
  });

  it("skips resolver when no sections exist", async () => {
    const tree: NavigationTree = {
      items: [{ title: "Guide", path: "/guide" }],
    };
    const resolver = vi.fn();

    const result = await resolveNavTree(tree, resolver);

    expect(resolver).not.toHaveBeenCalled();
    expect(result).toBe(tree); // same reference, no changes
  });

  it("synthesizes root parentScope for top-level sections", async () => {
    const tree: NavigationTree = {
      items: [{ title: "Overview", path: "/domains/billing/overview" }],
      scope: {
        path: "/domains/billing",
        title: "Billing",
        section: { kind: "domain", name: "billing" },
      },
      // No parentScope — this is a top-level section
    };
    const resolver = vi.fn().mockResolvedValue({
      "domain:default/billing": "/catalog/default/domain/billing/docs",
      "section:default/root": "/catalog/default/system/my-service/docs",
    });

    const result = await resolveNavTree(tree, resolver);

    expect(result.parentScope).toBeDefined();
    expect(result.parentScope!.title).toBe("Home");
    expect(result.parentScope!.href).toBe("/catalog/default/system/my-service/docs");
    expect(resolver).toHaveBeenCalledWith(expect.arrayContaining(["section:default/root"]));
  });

  it("keeps original path for unresolved nav refs", async () => {
    const tree: NavigationTree = {
      items: [
        {
          title: "Billing",
          path: "/domains/billing",
          section: { kind: "domain", name: "billing" },
        },
      ],
    };
    const resolver = vi.fn().mockResolvedValue({});

    const result = await resolveNavTree(tree, resolver);

    expect(result.items[0].href).toBeUndefined(); // unresolved, no href
    expect(result.items[0].path).toBe("/domains/billing"); // path preserved
  });
});

describe("resolveBreadcrumbs", () => {
  it("sets href on breadcrumbs with resolved refs", async () => {
    const breadcrumbs: Breadcrumb[] = [
      { title: "Home", path: "/" },
      { title: "Billing", path: "/domains/billing", section: { kind: "domain", name: "billing" } },
      { title: "API", path: "/domains/billing/api" },
    ];
    const resolver = vi.fn().mockResolvedValue({
      "domain:default/billing": "/catalog/default/domain/billing/docs",
    });

    const result = await resolveBreadcrumbs(breadcrumbs, resolver);

    expect(result[0].path).toBe("/"); // no section, unchanged
    expect(result[0].href).toBeUndefined();
    expect(result[1].href).toBe("/catalog/default/domain/billing/docs");
    expect(result[1].path).toBe("/domains/billing"); // path preserved
    expect(result[2].path).toBe("/domains/billing/api"); // no section, unchanged
    expect(result[2].href).toBeUndefined();
  });

  it("skips resolver when no breadcrumbs have sections", async () => {
    const breadcrumbs: Breadcrumb[] = [
      { title: "Home", path: "/" },
      { title: "Guide", path: "/guide" },
    ];
    const resolver = vi.fn();

    const result = await resolveBreadcrumbs(breadcrumbs, resolver);

    expect(resolver).not.toHaveBeenCalled();
    expect(result).toBe(breadcrumbs);
  });
});
