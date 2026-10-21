import { describe, it, expect, vi } from "vitest";
import {
  rewriteSectionRefLinks,
  rewriteLocalLinks,
  sectionRefString,
  ancestryHref,
  SectionLinks,
} from "./sectionRefs";
import { diagramShadowRoots } from "./diagram/source";
import { registerRwDiagram } from "./diagram/rwDiagramElement";
import type { SectionAncestry } from "../types";

async function linksFor(resolved: Record<string, string>): Promise<SectionLinks> {
  const links = new SectionLinks(() => Promise.resolve(resolved));
  await links.ensure(ANCESTRY);
  return links;
}

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

// A later response's ancestry: ANCESTRY plus sections it alone links to.
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

  it("appends a query-only subpath to the mapped section root without a slash", () => {
    const resolved = { "domain:default/billing": BILLING_BASE };
    expect(ancestryHref("domain:default/billing", "?tab=2", ANCESTRY, resolved)).toBe(
      `${BILLING_BASE}?tab=2`,
    );
  });

  it("appends a query-only subpath to an ancestor's subpath without a slash", () => {
    const resolved = { "domain:default/billing": BILLING_BASE };
    expect(ancestryHref("system:default/pay", "?tab=2", ANCESTRY, resolved)).toBe(
      `${BILLING_BASE}/pay?tab=2`,
    );
  });

  it("returns undefined when the ref is absent from the ancestry map", () => {
    expect(ancestryHref("system:default/unknown", "x", ANCESTRY, {})).toBeUndefined();
  });

  it("returns undefined when no ancestor in the chain is mapped", () => {
    expect(ancestryHref("system:default/pay", "config", ANCESTRY, {})).toBeUndefined();
  });

  it("falls through a null host base to the next mapped ancestor", () => {
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

  it("falls through an empty-string host base to the next mapped ancestor", () => {
    const resolved = {
      "system:default/pay": "",
      "domain:default/billing": BILLING_BASE,
    };
    expect(ancestryHref("system:default/pay", "config", ANCESTRY, resolved)).toBe(
      `${BILLING_BASE}/pay/config`,
    );
  });

  it("does not produce a doubled slash when the host base ends in '/'", () => {
    const resolved = { "domain:default/billing": `${BILLING_BASE}/` };
    expect(ancestryHref("domain:default/billing", "overview", ANCESTRY, resolved)).toBe(
      `${BILLING_BASE}/overview`,
    );
  });

  it("encodes the ancestor's page-path subpath but takes the link's subpath as a URL", () => {
    const ancestry: SectionAncestry = {
      "system:default/sys#1": [
        { sectionRef: "system:default/sys#1", subpath: "" },
        { sectionRef: "domain:default/billing", subpath: "100%/sys#1" },
      ],
    };
    const resolved = { "domain:default/billing": BILLING_BASE };
    expect(ancestryHref("system:default/sys#1", "c%23?x", ancestry, resolved)).toBe(
      `${BILLING_BASE}/100%25/sys%231/c%23?x`,
    );
  });
});

describe("rewriteSectionRefLinks", () => {
  it("rewrites a link via its nearest mapped section", async () => {
    const container = createContainer(
      `<a href="/billing/api" data-section-ref="domain:default/billing" data-section-path="api">Billing API</a>`,
    );
    const links = await linksFor({ "domain:default/billing": BILLING_BASE });

    rewriteSectionRefLinks([container], links, "", ANCESTRY);

    expect(container.querySelector("a")!.getAttribute("href")).toBe(`${BILLING_BASE}/api`);
  });

  it("resolves a link into an unmapped section via the mapped ancestor (#624)", async () => {
    // `pay` is not host-mapped; the link must fall through to the billing base.
    const container = createContainer(
      `<a href="/billing/pay" data-section-ref="system:default/pay" data-section-path="">pay overview</a>`,
    );
    const links = await linksFor({ "domain:default/billing": BILLING_BASE });

    rewriteSectionRefLinks([container], links, "/mount", ANCESTRY);

    expect(container.querySelector("a")!.getAttribute("href")).toBe(`${BILLING_BASE}/pay`);
  });

  it("treats a missing data-section-path as an empty subpath", async () => {
    const container = createContainer(
      `<a href="/billing" data-section-ref="domain:default/billing">Billing</a>`,
    );
    const links = await linksFor({ "domain:default/billing": BILLING_BASE });

    rewriteSectionRefLinks([container], links, "", ANCESTRY);

    expect(container.querySelector("a")!.getAttribute("href")).toBe(BILLING_BASE);
  });

  it("falls back to basePath when nothing in the chain resolves", async () => {
    const container = createContainer(
      `<a href="/billing/pay" data-section-ref="system:default/pay" data-section-path="">Pay</a>`,
    );
    const links = await linksFor({});

    rewriteSectionRefLinks([container], links, "/mount/base", ANCESTRY);

    expect(container.querySelector("a")!.getAttribute("href")).toBe("/mount/base");
  });

  it("falls back to local basePath + subpath when the host is MISCONFIGURED and maps no ancestor, not even root", async () => {
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
    const links = await linksFor({});

    rewriteSectionRefLinks([container], links, "/mount", ANCESTRY);

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
    const links = await linksFor({ "section:default/root": ROOT_BASE });

    rewriteSectionRefLinks([container], links, "/mount", ANCESTRY);

    expect(container.querySelector("a")!.getAttribute("href")).toBe(
      `${ROOT_BASE}/billing/pay/config`,
    );
  });

  it("preserves the URL fragment when resolving via a mapped ancestor", async () => {
    const container = createContainer(
      `<a href="/billing/api#endpoints" data-section-ref="domain:default/billing" data-section-path="api">Billing API</a>`,
    );
    const links = await linksFor({ "domain:default/billing": BILLING_BASE });

    rewriteSectionRefLinks([container], links, "", ANCESTRY);

    expect(container.querySelector("a")!.getAttribute("href")).toBe(
      `${BILLING_BASE}/api#endpoints`,
    );
  });

  it("preserves the URL fragment in the unmapped fallback", async () => {
    const container = createContainer(
      `<a href="/billing/api#endpoints" data-section-ref="domain:default/billing" data-section-path="api">Billing API</a>`,
    );
    const links = await linksFor({});

    rewriteSectionRefLinks([container], links, "/mount", ANCESTRY);

    expect(container.querySelector("a")!.getAttribute("href")).toBe("/mount/api#endpoints");
  });

  // The backend keeps "?…" in data-section-path and drops "#…", so these tests
  // put the query there and the fragment on the href.
  it("preserves the query (via subpath) and fragment (via href) together", async () => {
    const container = createContainer(
      `<a href="/billing/pay#sec" data-section-ref="domain:default/billing" data-section-path="pay?tab=2">Pay</a>`,
    );
    const links = await linksFor({ "domain:default/billing": BILLING_BASE });

    rewriteSectionRefLinks([container], links, "", ANCESTRY);

    expect(container.querySelector("a")!.getAttribute("href")).toBe(
      `${BILLING_BASE}/pay?tab=2#sec`,
    );
  });

  it("preserves a query-only subpath, with no doubling", async () => {
    const container = createContainer(
      `<a href="/billing/pay" data-section-ref="domain:default/billing" data-section-path="pay?tab=2">Pay</a>`,
    );
    const links = await linksFor({ "domain:default/billing": BILLING_BASE });

    rewriteSectionRefLinks([container], links, "", ANCESTRY);

    expect(container.querySelector("a")!.getAttribute("href")).toBe(`${BILLING_BASE}/pay?tab=2`);
  });

  it("preserves the query (in subpath) in the unmapped fallback", async () => {
    const container = createContainer(
      `<a href="/billing/api" data-section-ref="domain:default/billing" data-section-path="api?tab=2">Billing API</a>`,
    );
    const links = await linksFor({});

    rewriteSectionRefLinks([container], links, "/mount", ANCESTRY);

    expect(container.querySelector("a")!.getAttribute("href")).toBe("/mount/api?tab=2");
  });

  it("resolves two links in the same section to distinct hrefs", async () => {
    const container = createContainer(`
      <a data-section-ref="domain:default/billing" data-section-path="s5/s6/page">a</a>
      <a data-section-ref="domain:default/billing" data-section-path="s5/s7/page">b</a>
    `);
    const links = await linksFor({ "domain:default/billing": "/host/billing" });

    rewriteSectionRefLinks([container], links, "/mount", ANCESTRY);

    const anchors = container.querySelectorAll("a");
    expect(anchors[0].getAttribute("href")).toBe("/host/billing/s5/s6/page");
    expect(anchors[1].getAttribute("href")).toBe("/host/billing/s5/s7/page");
  });

  it("rewrites a link inside a diagram shadow root", async () => {
    registerRwDiagram();
    const container = document.createElement("div");
    document.body.appendChild(container);
    container.innerHTML =
      `<rw-diagram><svg><a href="/billing/api" data-section-ref="domain:default/billing" ` +
      `data-section-path="api">Billing API</a></svg></rw-diagram>`;
    const links = await linksFor({ "domain:default/billing": BILLING_BASE });

    rewriteSectionRefLinks([container, ...diagramShadowRoots(container)], links, "", ANCESTRY);

    const anchor = container.querySelector("rw-diagram")!.shadowRoot!.querySelector("a")!;
    expect(anchor.getAttribute("href")).toBe(`${BILLING_BASE}/api`);
  });
});

describe("rewriteLocalLinks", () => {
  it("maps each section link once, so a rerun over kept nodes is a no-op", () => {
    const container = createContainer(
      `<a href="/billing/api#top" data-section-ref="domain:default/billing" data-section-path="api">Billing API</a>` +
        `<a href="/elsewhere">plain</a>`,
    );
    const toHref = (href: string) => `/mount${href}`;

    rewriteLocalLinks([container], toHref);
    rewriteLocalLinks([container], toHref);

    const [section, plain] = container.querySelectorAll("a");
    expect(section.getAttribute("href")).toBe("/mount/billing/api#top");
    expect(plain.getAttribute("href")).toBe("/elsewhere");
  });

  it("restores a link its mapper declines", () => {
    const container = createContainer(
      `<a href="/billing/invoices" data-section-ref="domain:default/billing" data-section-path="invoices">Invoices</a>`,
    );

    rewriteLocalLinks([container], (href) => `/mount${href}`);
    rewriteLocalLinks([container], () => null);

    expect(container.querySelector("a")!.getAttribute("href")).toBe("/billing/invoices");
  });
});

describe("SectionLinks", () => {
  it("asks the host about every ref in the chains, in one call", async () => {
    const resolver = vi.fn().mockResolvedValue({ "domain:default/billing": BILLING_BASE });
    const links = new SectionLinks(resolver);

    await links.ensure(ANCESTRY);

    expect(resolver).toHaveBeenCalledTimes(1);
    expect(new Set(resolver.mock.calls[0][0] as string[])).toEqual(
      new Set(["system:default/pay", "domain:default/billing", "section:default/root"]),
    );
    expect(links.href("system:default/pay", "config", ANCESTRY)).toBe(`${BILLING_BASE}/pay/config`);
  });

  it("asks only about refs the host has not answered, mapped or not", async () => {
    const resolver = vi.fn().mockResolvedValue({ "domain:default/billing": BILLING_BASE });
    const links = new SectionLinks(resolver);

    await links.ensure(ANCESTRY);
    await links.ensure(ANCESTRY);
    await links.ensure(ANCESTRY_WITH_EXTRA);

    expect(resolver).toHaveBeenCalledTimes(2);
    expect(new Set(resolver.mock.calls[1][0] as string[])).toEqual(
      new Set(["domain:default/unrelated", "system:default/other"]),
    );
  });

  it("asks again about refs whose resolution failed", async () => {
    const resolver = vi
      .fn()
      .mockRejectedValueOnce(new Error("host resolver failed"))
      .mockResolvedValue({ "domain:default/billing": BILLING_BASE });
    const links = new SectionLinks(resolver);

    await links.ensure(ANCESTRY);
    expect(links.href("domain:default/billing", "", ANCESTRY)).toBeUndefined();

    await links.ensure(ANCESTRY);
    expect(resolver).toHaveBeenCalledTimes(2);
    expect(links.href("domain:default/billing", "", ANCESTRY)).toBe(BILLING_BASE);
  });

  it("resolves nothing without a host resolver", async () => {
    const links = new SectionLinks();

    await links.ensure(ANCESTRY);

    expect(links.enabled).toBe(false);
    expect(links.href("domain:default/billing", "", ANCESTRY)).toBeUndefined();
  });
});
