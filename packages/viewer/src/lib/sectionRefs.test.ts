import { describe, it, expect, vi } from "vitest";
import {
  rewriteSectionRefLinks,
  sectionRefString,
  ancestryHref,
  resolveNavTree,
  resolveBreadcrumbs,
} from "./sectionRefs";
import { diagramShadowRoots } from "./diagram/source";
import { registerRwDiagram } from "./diagram/rwDiagramElement";
import type { NavigationTree, Breadcrumb, SectionInfo, SectionAncestry } from "../types";

function createContainer(html: string): HTMLElement {
  const el = document.createElement("div");
  el.innerHTML = html;
  return el;
}

// Shared fixture mirroring the e2e: a `billing` domain containing a nested
// `pay` system. Each section keys to its own chain (itself first, root last).
const ANCESTRY: SectionAncestry = {
  "domain:default/billing": [
    { sectionRef: "domain:default/billing", subpath: "" },
    { sectionRef: "section:default/root", subpath: "billing" },
  ],
  "system:default/pay": [
    { sectionRef: "system:default/pay", subpath: "" },
    { sectionRef: "domain:default/billing", subpath: "pay" },
    { sectionRef: "section:default/root", subpath: "billing/pay" },
  ],
  "section:default/root": [{ sectionRef: "section:default/root", subpath: "" }],
};

const BILLING_BASE = "/catalog/default/domain/billing/docs";

// Base URL for the site-root ref, used by the contract-path tests below where
// only root is host-mapped (nearest + intermediate ancestors unmapped).
const ROOT_BASE = "/catalog/default/system/my-service/docs";

// An ancestry map that also contains sections nowhere referenced on the
// page/nav/crumbs under test (e.g. other sections resolved elsewhere on the
// site). These extra refs must never be sent to the resolver.
const ANCESTRY_WITH_EXTRA: SectionAncestry = {
  ...ANCESTRY,
  "domain:default/unrelated": [
    { sectionRef: "domain:default/unrelated", subpath: "" },
    { sectionRef: "section:default/root", subpath: "unrelated" },
  ],
  "system:default/other": [
    { sectionRef: "system:default/other", subpath: "" },
    { sectionRef: "domain:default/unrelated", subpath: "other" },
    { sectionRef: "section:default/root", subpath: "unrelated/other" },
  ],
};

describe("sectionRefString", () => {
  it("builds a ref string from section info", () => {
    expect(sectionRefString({ kind: "domain", namespace: "default", name: "billing" })).toBe(
      "domain:default/billing",
    );
    expect(sectionRefString({ kind: "domain", namespace: "payments", name: "billing" })).toBe(
      "domain:payments/billing",
    );
  });
});

describe("ancestryHref", () => {
  it("resolves the nearest ref when it is host-mapped", () => {
    const resolved = { "system:default/pay": "/host/pay" };
    // pay's own subpath is empty; the link targets pay/config.
    expect(ancestryHref("system:default/pay", "config", ANCESTRY, resolved)).toBe(
      "/host/pay/config",
    );
  });

  it("walks past an unmapped nearest section to the first mapped ancestor (#624)", () => {
    // `pay` is unmapped, so resolution falls through to billing, joining
    // billing's subpath-to-pay ("pay") with the link's own remainder.
    const resolved = { "domain:default/billing": BILLING_BASE };
    expect(ancestryHref("system:default/pay", "config", ANCESTRY, resolved)).toBe(
      `${BILLING_BASE}/pay/config`,
    );
  });

  it("omits the slash when the joined subpath is empty (target is the mapped section root)", () => {
    const resolved = { "domain:default/billing": BILLING_BASE };
    expect(ancestryHref("domain:default/billing", "", ANCESTRY, resolved)).toBe(BILLING_BASE);
  });

  it("joins only the ancestor's own subpath when the link subpath is empty", () => {
    // A link to `pay`'s root, pay unmapped -> billing base + "pay".
    const resolved = { "domain:default/billing": BILLING_BASE };
    expect(ancestryHref("system:default/pay", "", ANCESTRY, resolved)).toBe(`${BILLING_BASE}/pay`);
  });

  it("returns undefined when the ref is absent from the ancestry map", () => {
    expect(ancestryHref("system:default/unknown", "x", ANCESTRY, {})).toBeUndefined();
    expect(ancestryHref(undefined, "x", ANCESTRY, {})).toBeUndefined();
    expect(ancestryHref("system:default/pay", "x", undefined, {})).toBeUndefined();
  });

  it("returns undefined when no ancestor in the chain is mapped", () => {
    expect(ancestryHref("system:default/pay", "config", ANCESTRY, {})).toBeUndefined();
  });

  it("falls through a null host base to the next mapped ancestor (#5)", () => {
    // A JS host isn't type-checked against Record<string, string> and may
    // return null for a ref it declines to map.
    const resolved = {
      "system:default/pay": null as unknown as string,
      "domain:default/billing": BILLING_BASE,
    };
    expect(ancestryHref("system:default/pay", "config", ANCESTRY, resolved)).toBe(
      `${BILLING_BASE}/pay/config`,
    );
  });

  it("falls through an empty-string host base to the next mapped ancestor (#5)", () => {
    const resolved = {
      "system:default/pay": "",
      "domain:default/billing": BILLING_BASE,
    };
    expect(ancestryHref("system:default/pay", "config", ANCESTRY, resolved)).toBe(
      `${BILLING_BASE}/pay/config`,
    );
  });

  it("does not produce a doubled slash when the host base ends in '/' (#8)", () => {
    const resolved = { "domain:default/billing": `${BILLING_BASE}/` };
    expect(ancestryHref("domain:default/billing", "overview", ANCESTRY, resolved)).toBe(
      `${BILLING_BASE}/overview`,
    );
  });
});

describe("rewriteSectionRefLinks", () => {
  it("rewrites a link via its nearest mapped section", async () => {
    const container = createContainer(
      `<a href="/billing/api" data-section-ref="domain:default/billing" data-section-path="api">Billing API</a>`,
    );
    const resolver = vi.fn().mockResolvedValue({ "domain:default/billing": BILLING_BASE });

    await rewriteSectionRefLinks([container], resolver, () => "", ANCESTRY);

    expect(container.querySelector("a")!.getAttribute("href")).toBe(`${BILLING_BASE}/api`);
  });

  it("resolves a link into an unmapped section via the mapped ancestor (#624)", async () => {
    // `pay` is not host-mapped; the link must fall through to the billing base.
    const container = createContainer(
      `<a href="/billing/pay" data-section-ref="system:default/pay" data-section-path="">pay overview</a>`,
    );
    const resolver = vi.fn().mockResolvedValue({ "domain:default/billing": BILLING_BASE });

    await rewriteSectionRefLinks([container], resolver, () => "/mount", ANCESTRY);

    expect(container.querySelector("a")!.getAttribute("href")).toBe(`${BILLING_BASE}/pay`);
  });

  it("treats a missing data-section-path as an empty subpath", async () => {
    const container = createContainer(
      `<a href="/billing" data-section-ref="domain:default/billing">Billing</a>`,
    );
    const resolver = vi.fn().mockResolvedValue({ "domain:default/billing": BILLING_BASE });

    await rewriteSectionRefLinks([container], resolver, () => "", ANCESTRY);

    expect(container.querySelector("a")!.getAttribute("href")).toBe(BILLING_BASE);
  });

  it("falls back to basePath when nothing in the chain resolves", async () => {
    const container = createContainer(
      `<a href="/billing/pay" data-section-ref="system:default/pay" data-section-path="">Pay</a>`,
    );
    const resolver = vi.fn().mockResolvedValue({});

    await rewriteSectionRefLinks([container], resolver, () => "/mount/base", ANCESTRY);

    expect(container.querySelector("a")!.getAttribute("href")).toBe("/mount/base");
  });

  it("falls back to local basePath + subpath when the host is MISCONFIGURED and maps no ancestor, not even root (#4)", async () => {
    // The host is required to map the site-root ref (see SectionRefResolver's
    // contract), so every chain should always bottom out there. An empty
    // resolver simulates a host that maps NOTHING — including root — which is
    // a misconfiguration, not the intended cross-entity path (that's covered
    // below, "resolves via root ..."). This only documents the last-resort
    // local-routing fallback: the link's own subpath ("api") must still
    // survive it — distinct unmapped links must not all collapse to the bare
    // mount root.
    const container = createContainer(
      `<a href="/billing/api" data-section-ref="domain:default/billing" data-section-path="api">Billing API</a>`,
    );
    const resolver = vi.fn().mockResolvedValue({});

    await rewriteSectionRefLinks([container], resolver, () => "/mount", ANCESTRY);

    expect(container.querySelector("a")!.getAttribute("href")).toBe("/mount/api");
  });

  it("resolves via root when the nearest ref AND an intermediate ancestor are both unmapped, only root mapped (#624)", async () => {
    // Contract path: `pay` (nearest) and `billing` (intermediate) are both
    // unmapped; only the site-root is host-mapped, as the host is required to
    // provide. The chain for `pay` is [pay, billing, root] with root's own
    // subpath-to-pay being "billing/pay" (see ANCESTRY above); joining that
    // with the link's own subpath ("config") must survive the full walk to
    // root, not just the immediate parent.
    const container = createContainer(
      `<a href="/billing/pay/config" data-section-ref="system:default/pay" data-section-path="config">Pay Config</a>`,
    );
    const resolver = vi.fn().mockResolvedValue({ "section:default/root": ROOT_BASE });

    await rewriteSectionRefLinks([container], resolver, () => "/mount", ANCESTRY);

    expect(container.querySelector("a")!.getAttribute("href")).toBe(
      `${ROOT_BASE}/billing/pay/config`,
    );
  });

  it("preserves the URL fragment when resolving via a mapped ancestor (#3)", async () => {
    const container = createContainer(
      `<a href="/billing/api#endpoints" data-section-ref="domain:default/billing" data-section-path="api">Billing API</a>`,
    );
    const resolver = vi.fn().mockResolvedValue({ "domain:default/billing": BILLING_BASE });

    await rewriteSectionRefLinks([container], resolver, () => "", ANCESTRY);

    expect(container.querySelector("a")!.getAttribute("href")).toBe(
      `${BILLING_BASE}/api#endpoints`,
    );
  });

  it("preserves the URL fragment in the unmapped fallback (#3)", async () => {
    const container = createContainer(
      `<a href="/billing/api#endpoints" data-section-ref="domain:default/billing" data-section-path="api">Billing API</a>`,
    );
    const resolver = vi.fn().mockResolvedValue({});

    await rewriteSectionRefLinks([container], resolver, () => "/mount", ANCESTRY);

    expect(container.querySelector("a")!.getAttribute("href")).toBe("/mount/api#endpoints");
  });

  // The backend's Sections::find strips only "#…", not "?…", so a query string
  // stays inside data-section-path (the `subpath`) and is preserved without any
  // re-append; only the fragment is read back off the href. These tests put the
  // query in data-section-path and the fragment on the href to prove that.
  it("preserves the query (via subpath) and fragment (via href) together (#12)", async () => {
    const container = createContainer(
      `<a href="/billing/pay#sec" data-section-ref="domain:default/billing" data-section-path="pay?tab=2">Pay</a>`,
    );
    const resolver = vi.fn().mockResolvedValue({ "domain:default/billing": BILLING_BASE });

    await rewriteSectionRefLinks([container], resolver, () => "", ANCESTRY);

    expect(container.querySelector("a")!.getAttribute("href")).toBe(
      `${BILLING_BASE}/pay?tab=2#sec`,
    );
  });

  it("preserves a query-only subpath, with no doubling (#12)", async () => {
    const container = createContainer(
      `<a href="/billing/pay" data-section-ref="domain:default/billing" data-section-path="pay?tab=2">Pay</a>`,
    );
    const resolver = vi.fn().mockResolvedValue({ "domain:default/billing": BILLING_BASE });

    await rewriteSectionRefLinks([container], resolver, () => "", ANCESTRY);

    expect(container.querySelector("a")!.getAttribute("href")).toBe(`${BILLING_BASE}/pay?tab=2`);
  });

  it("preserves the query (in subpath) in the unmapped fallback (#12)", async () => {
    const container = createContainer(
      `<a href="/billing/api" data-section-ref="domain:default/billing" data-section-path="api?tab=2">Billing API</a>`,
    );
    const resolver = vi.fn().mockResolvedValue({});

    await rewriteSectionRefLinks([container], resolver, () => "/mount", ANCESTRY);

    expect(container.querySelector("a")!.getAttribute("href")).toBe("/mount/api?tab=2");
  });

  it("does not write stale results after the caller aborts (#2)", async () => {
    const container = createContainer(
      `<a href="/billing/api" data-section-ref="domain:default/billing" data-section-path="api">Billing API</a>`,
    );
    const originalHref = container.querySelector("a")!.getAttribute("href");
    let release: (value: Record<string, string>) => void = () => {};
    const resolver = vi.fn(
      () =>
        new Promise<Record<string, string>>((r) => {
          release = r;
        }),
    );

    const controller = new AbortController();
    const call = rewriteSectionRefLinks(
      [container],
      resolver,
      () => "/mount",
      ANCESTRY,
      controller.signal,
    );

    controller.abort();
    release({ "domain:default/billing": BILLING_BASE });
    await call;

    expect(container.querySelector("a")!.getAttribute("href")).toBe(originalHref);
  });

  it("writes results when the signal is not aborted (control for #2)", async () => {
    const container = createContainer(
      `<a href="/billing/api" data-section-ref="domain:default/billing" data-section-path="api">Billing API</a>`,
    );
    let release: (value: Record<string, string>) => void = () => {};
    const resolver = vi.fn(
      () =>
        new Promise<Record<string, string>>((r) => {
          release = r;
        }),
    );

    const controller = new AbortController();
    const call = rewriteSectionRefLinks(
      [container],
      resolver,
      () => "/mount",
      ANCESTRY,
      controller.signal,
    );

    release({ "domain:default/billing": BILLING_BASE });
    await call;

    expect(container.querySelector("a")!.getAttribute("href")).toBe(`${BILLING_BASE}/api`);
  });

  it("skips a container with no section-ref links", async () => {
    const container = createContainer('<a href="/about">About</a>');
    const resolver = vi.fn();

    await rewriteSectionRefLinks([container], resolver, () => "", ANCESTRY);

    expect(resolver).not.toHaveBeenCalled();
  });

  it("resolves two links in the same section to distinct hrefs in one resolver call", async () => {
    const container = createContainer(`
      <a data-section-ref="domain:default/billing" data-section-path="s5/s6/page">a</a>
      <a data-section-ref="domain:default/billing" data-section-path="s5/s7/page">b</a>
    `);
    const resolver = vi.fn().mockResolvedValue({ "domain:default/billing": "/host/billing" });

    await rewriteSectionRefLinks([container], resolver, () => "/mount", ANCESTRY);

    const links = container.querySelectorAll("a");
    expect(links[0].getAttribute("href")).toBe("/host/billing/s5/s6/page");
    expect(links[1].getAttribute("href")).toBe("/host/billing/s5/s7/page");
    expect(resolver).toHaveBeenCalledTimes(1);
  });

  it("asks the resolver only about the present link's ancestry chain, not the whole ancestry map (#2)", async () => {
    const container = createContainer(
      `<a href="/billing/api" data-section-ref="domain:default/billing" data-section-path="api">Billing API</a>`,
    );
    const resolver = vi.fn().mockResolvedValue({ "domain:default/billing": BILLING_BASE });

    await rewriteSectionRefLinks([container], resolver, () => "", ANCESTRY_WITH_EXTRA);

    expect(resolver).toHaveBeenCalledTimes(1);
    const refs = resolver.mock.calls[0][0] as string[];
    expect(new Set(refs)).toEqual(new Set(["domain:default/billing", "section:default/root"]));
    expect(refs).not.toContain("domain:default/unrelated");
    expect(refs).not.toContain("system:default/other");
  });

  it("still asks about the full chain of an unmapped present ref (walk-through, #2)", async () => {
    const container = createContainer(
      `<a href="/billing/pay" data-section-ref="system:default/pay" data-section-path="">pay overview</a>`,
    );
    const resolver = vi.fn().mockResolvedValue({ "domain:default/billing": BILLING_BASE });

    await rewriteSectionRefLinks([container], resolver, () => "/mount", ANCESTRY_WITH_EXTRA);

    const refs = resolver.mock.calls[0][0] as string[];
    // pay's whole chain (itself, billing, root) is offered so the walk can
    // fall through to billing even though pay itself is unmapped.
    expect(new Set(refs)).toEqual(
      new Set(["system:default/pay", "domain:default/billing", "section:default/root"]),
    );
    expect(refs).not.toContain("domain:default/unrelated");
  });

  it("rewrites a link inside a diagram shadow root", async () => {
    registerRwDiagram();
    const container = document.createElement("div");
    document.body.appendChild(container);
    container.innerHTML =
      `<rw-diagram><svg><a href="/billing/api" data-section-ref="domain:default/billing" ` +
      `data-section-path="api">Billing API</a></svg></rw-diagram>`;
    const resolver = vi.fn().mockResolvedValue({ "domain:default/billing": BILLING_BASE });

    await rewriteSectionRefLinks(
      [container, ...diagramShadowRoots(container)],
      resolver,
      () => "",
      ANCESTRY,
    );

    const anchor = container.querySelector("rw-diagram")!.shadowRoot!.querySelector("a")!;
    expect(anchor.getAttribute("href")).toBe(`${BILLING_BASE}/api`);
  });
});

describe("resolveNavTree", () => {
  it("rewrites nav item paths with resolved refs", async () => {
    const tree: NavigationTree = {
      items: [
        {
          title: "Billing",
          path: "/billing",
          section: { kind: "domain", namespace: "default", name: "billing" },
          children: [
            { title: "API", path: "/billing/api" },
            {
              title: "Pay",
              path: "/billing/pay",
              section: { kind: "system", namespace: "default", name: "pay" },
            },
          ],
        },
        { title: "Guide", path: "/guide" },
      ],
      sectionAncestry: ANCESTRY,
    };
    const resolver = vi.fn().mockResolvedValue({
      "domain:default/billing": BILLING_BASE,
      "system:default/pay": "/catalog/default/system/pay/docs",
    });

    const result = await resolveNavTree(tree, resolver);

    expect(result.items[0].href).toBe(BILLING_BASE);
    expect(result.items[0].path).toBe("/billing"); // path preserved for keying/active state
    expect(result.items[0].children![0].href).toBeUndefined(); // no section, no href
    expect(result.items[0].children![0].path).toBe("/billing/api");
    expect(result.items[0].children![1].href).toBe("/catalog/default/system/pay/docs");
    expect(result.items[1].href).toBeUndefined(); // no section, no href
    expect(result.items[1].path).toBe("/guide");
  });

  it("resolves an unmapped section nav item via its mapped ancestor (#624)", async () => {
    const tree: NavigationTree = {
      items: [
        {
          title: "Pay",
          path: "/billing/pay",
          section: { kind: "system", namespace: "default", name: "pay" },
        },
      ],
      sectionAncestry: ANCESTRY,
    };
    // pay unmapped; only billing is host-mapped.
    const resolver = vi.fn().mockResolvedValue({ "domain:default/billing": BILLING_BASE });

    const result = await resolveNavTree(tree, resolver);

    expect(result.items[0].href).toBe(`${BILLING_BASE}/pay`);
  });

  it("rewrites scope and parentScope via their own refs", async () => {
    const tree: NavigationTree = {
      items: [],
      scope: {
        path: "/billing",
        title: "Billing",
        section: { kind: "domain", namespace: "default", name: "billing" },
      },
      parentScope: {
        path: "/",
        title: "Home",
        section: { kind: "section", namespace: "default", name: "root" },
      },
      sectionAncestry: ANCESTRY,
    };
    const resolver = vi.fn().mockResolvedValue({ "domain:default/billing": BILLING_BASE });

    const result = await resolveNavTree(tree, resolver);

    expect(result.scope!.href).toBe(BILLING_BASE);
    expect(result.scope!.path).toBe("/billing"); // path preserved
    expect(result.parentScope!.href).toBeUndefined(); // root unmapped, no href
    expect(result.parentScope!.path).toBe("/"); // path preserved
  });

  it("skips resolver when the tree carries no ancestry map", async () => {
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
      items: [{ title: "Overview", path: "/billing/overview" }],
      scope: {
        path: "/billing",
        title: "Billing",
        section: { kind: "domain", namespace: "default", name: "billing" },
      },
      // No parentScope — this is a top-level section.
      sectionAncestry: ANCESTRY,
    };
    const resolver = vi.fn().mockResolvedValue({
      "domain:default/billing": BILLING_BASE,
      "section:default/root": "/catalog/default/system/my-service/docs",
    });

    const result = await resolveNavTree(tree, resolver);

    expect(result.parentScope).toBeDefined();
    expect(result.parentScope!.title).toBe("Home");
    expect(result.parentScope!.href).toBe("/catalog/default/system/my-service/docs");
  });

  it("synthesizes root parentScope using the scope's namespace", async () => {
    // Custom-namespace site: the synthesized root inherits the scope's
    // namespace so its ref matches the right catalog entity.
    const ancestry: SectionAncestry = {
      "domain:payments/billing": [
        { sectionRef: "domain:payments/billing", subpath: "" },
        { sectionRef: "section:payments/root", subpath: "billing" },
      ],
      "section:payments/root": [{ sectionRef: "section:payments/root", subpath: "" }],
    };
    const tree: NavigationTree = {
      items: [{ title: "Overview", path: "/billing/overview" }],
      scope: {
        path: "/billing",
        title: "Billing",
        section: { kind: "domain", namespace: "payments", name: "billing" },
      },
      sectionAncestry: ancestry,
    };
    const resolver = vi.fn().mockResolvedValue({
      "section:payments/root": "/catalog/payments/system/my-service/docs",
    });

    const result = await resolveNavTree(tree, resolver);

    expect(result.parentScope!.href).toBe("/catalog/payments/system/my-service/docs");
  });

  it("synthesizes root parentScope with 'default' when scope.section.namespace is missing", async () => {
    // Backward-compat: an older backend may serialize scope.section without the
    // namespace field. The fallback coerces to "default" rather than producing
    // "section:undefined/root".
    const ancestry: SectionAncestry = {
      "section:default/root": [{ sectionRef: "section:default/root", subpath: "" }],
    };
    const tree: NavigationTree = {
      items: [{ title: "Overview", path: "/billing/overview" }],
      scope: {
        path: "/billing",
        title: "Billing",
        // Cast around the type-checker to mimic an older backend's payload.
        section: { kind: "domain", name: "billing" } as unknown as SectionInfo,
      },
      sectionAncestry: ancestry,
    };
    const resolver = vi.fn().mockResolvedValue({
      "section:default/root": "/catalog/default/system/my-service/docs",
    });

    const result = await resolveNavTree(tree, resolver);

    expect(result.parentScope!.href).toBe("/catalog/default/system/my-service/docs");
  });

  it("keeps href undefined for unresolved nav refs", async () => {
    const tree: NavigationTree = {
      items: [
        {
          title: "Billing",
          path: "/billing",
          section: { kind: "domain", namespace: "default", name: "billing" },
        },
      ],
      sectionAncestry: ANCESTRY,
    };
    const resolver = vi.fn().mockResolvedValue({});

    const result = await resolveNavTree(tree, resolver);

    expect(result.items[0].href).toBeUndefined();
    expect(result.items[0].path).toBe("/billing"); // path preserved
  });

  it("asks the resolver only about the chains of refs present in the tree, not the whole ancestry map (#2)", async () => {
    const tree: NavigationTree = {
      items: [
        {
          title: "Billing",
          path: "/billing",
          section: { kind: "domain", namespace: "default", name: "billing" },
        },
      ],
      sectionAncestry: ANCESTRY_WITH_EXTRA,
    };
    const resolver = vi.fn().mockResolvedValue({ "domain:default/billing": BILLING_BASE });

    await resolveNavTree(tree, resolver);

    expect(resolver).toHaveBeenCalledTimes(1);
    const refs = resolver.mock.calls[0][0] as string[];
    expect(new Set(refs)).toEqual(new Set(["domain:default/billing", "section:default/root"]));
    expect(refs).not.toContain("domain:default/unrelated");
    expect(refs).not.toContain("system:default/other");
  });

  it("still asks about the full chain of an unmapped present nav ref (walk-through, #2)", async () => {
    const tree: NavigationTree = {
      items: [
        {
          title: "Pay",
          path: "/billing/pay",
          section: { kind: "system", namespace: "default", name: "pay" },
        },
      ],
      sectionAncestry: ANCESTRY_WITH_EXTRA,
    };
    const resolver = vi.fn().mockResolvedValue({ "domain:default/billing": BILLING_BASE });

    await resolveNavTree(tree, resolver);

    const refs = resolver.mock.calls[0][0] as string[];
    expect(new Set(refs)).toEqual(
      new Set(["system:default/pay", "domain:default/billing", "section:default/root"]),
    );
    expect(refs).not.toContain("domain:default/unrelated");
  });

  it("includes scope and parentScope refs when collecting present refs (#2)", async () => {
    const tree: NavigationTree = {
      items: [],
      scope: {
        path: "/billing",
        title: "Billing",
        section: { kind: "domain", namespace: "default", name: "billing" },
      },
      parentScope: {
        path: "/",
        title: "Home",
        section: { kind: "section", namespace: "default", name: "root" },
      },
      sectionAncestry: ANCESTRY_WITH_EXTRA,
    };
    const resolver = vi.fn().mockResolvedValue({ "domain:default/billing": BILLING_BASE });

    await resolveNavTree(tree, resolver);

    const refs = resolver.mock.calls[0][0] as string[];
    expect(new Set(refs)).toEqual(new Set(["domain:default/billing", "section:default/root"]));
    expect(refs).not.toContain("domain:default/unrelated");
  });
});

describe("resolveBreadcrumbs", () => {
  it("sets href on breadcrumbs via their nearest ref and subpath", async () => {
    const breadcrumbs: Breadcrumb[] = [
      { title: "Home", path: "/", sectionRef: "section:default/root", subpath: "" },
      {
        title: "Billing",
        path: "/billing",
        sectionRef: "domain:default/billing",
        subpath: "",
      },
      { title: "Pay", path: "/billing/pay", sectionRef: "system:default/pay", subpath: "" },
    ];
    const resolver = vi.fn().mockResolvedValue({ "domain:default/billing": BILLING_BASE });

    const result = await resolveBreadcrumbs(breadcrumbs, ANCESTRY, resolver);

    // Home's nearest ref (root) is unmapped -> no href.
    expect(result[0].href).toBeUndefined();
    // Billing is mapped directly.
    expect(result[1].href).toBe(BILLING_BASE);
    expect(result[1].path).toBe("/billing"); // path preserved
    // Pay is unmapped -> resolves via billing ancestor (#624).
    expect(result[2].href).toBe(`${BILLING_BASE}/pay`);
  });

  it("resolves via root when the nearest ref AND an intermediate ancestor are both unmapped, only root mapped (#624)", async () => {
    // Same contract path as the rewriteSectionRefLinks case above: pay and
    // billing both unmapped, only root mapped. Root's own subpath-to-pay
    // ("billing/pay") joined with the breadcrumb's own (empty) subpath must
    // survive the full walk.
    const breadcrumbs: Breadcrumb[] = [
      { title: "Pay", path: "/billing/pay", sectionRef: "system:default/pay", subpath: "" },
    ];
    const resolver = vi.fn().mockResolvedValue({ "section:default/root": ROOT_BASE });

    const result = await resolveBreadcrumbs(breadcrumbs, ANCESTRY, resolver);

    expect(result[0].href).toBe(`${ROOT_BASE}/billing/pay`);
  });

  it("skips the resolver when there is no ancestry map", async () => {
    const breadcrumbs: Breadcrumb[] = [
      { title: "Home", path: "/" },
      { title: "Guide", path: "/guide" },
    ];
    const resolver = vi.fn();

    const result = await resolveBreadcrumbs(breadcrumbs, undefined, resolver);

    expect(resolver).not.toHaveBeenCalled();
    expect(result).toBe(breadcrumbs);
  });

  it("asks the resolver only about the chains of refs present in the breadcrumbs, not the whole ancestry map (#2)", async () => {
    const breadcrumbs: Breadcrumb[] = [
      {
        title: "Billing",
        path: "/billing",
        sectionRef: "domain:default/billing",
        subpath: "",
      },
    ];
    const resolver = vi.fn().mockResolvedValue({ "domain:default/billing": BILLING_BASE });

    await resolveBreadcrumbs(breadcrumbs, ANCESTRY_WITH_EXTRA, resolver);

    expect(resolver).toHaveBeenCalledTimes(1);
    const refs = resolver.mock.calls[0][0] as string[];
    expect(new Set(refs)).toEqual(new Set(["domain:default/billing", "section:default/root"]));
    expect(refs).not.toContain("domain:default/unrelated");
    expect(refs).not.toContain("system:default/other");
  });

  it("still asks about the full chain of an unmapped present breadcrumb ref (walk-through, #2)", async () => {
    const breadcrumbs: Breadcrumb[] = [
      { title: "Pay", path: "/billing/pay", sectionRef: "system:default/pay", subpath: "" },
    ];
    const resolver = vi.fn().mockResolvedValue({ "domain:default/billing": BILLING_BASE });

    await resolveBreadcrumbs(breadcrumbs, ANCESTRY_WITH_EXTRA, resolver);

    const refs = resolver.mock.calls[0][0] as string[];
    expect(new Set(refs)).toEqual(
      new Set(["system:default/pay", "domain:default/billing", "section:default/root"]),
    );
    expect(refs).not.toContain("domain:default/unrelated");
  });
});
