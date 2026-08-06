import { describe, it, expect } from "vitest";
import { groupNavItems, toSelfItem } from "./navigation";
import type { NavItem, ScopeInfo } from "../types";

const plain: NavItem = { title: "Guide", path: "/guide" };
const domain: NavItem = {
  title: "Payments",
  path: "/payments",
  section: { kind: "domain", namespace: "default", name: "payments" },
};
// A scope page always carries a section — the root's is kind "section".
const selfRow: NavItem = {
  title: "Billing",
  path: "/billing",
  section: { kind: "domain", namespace: "default", name: "billing" },
};

describe("groupNavItems", () => {
  it("puts the self item first, ahead of ungrouped items", () => {
    const groups = groupNavItems([plain], selfRow);

    expect(groups[0].label).toBeNull();
    expect(groups[0].items.map((i) => i.path)).toEqual(["/billing", "/guide"]);
  });

  it("keeps the self item out of its own kind group", () => {
    const groups = groupNavItems([domain], selfRow);

    // Without the bypass the self row lands under "Domains", below its children.
    expect(groups[0].label).toBeNull();
    expect(groups[0].items).toEqual([selfRow]);
    expect(groups[1].label).toBe("Domains");
    expect(groups[1].items).toEqual([domain]);
  });

  it("adds no group beyond the self item's and the kind's", () => {
    const groups = groupNavItems([domain], selfRow);

    // The test above pins groups[0] and groups[1] but never bounds the length.
    expect(groups).toHaveLength(2);
  });

  it("is unchanged when no self item is given", () => {
    const groups = groupNavItems([plain, domain]);

    expect(groups[0].label).toBeNull();
    expect(groups[0].items).toEqual([plain]);
    expect(groups[1].label).toBe("Domains");
  });
});

describe("toSelfItem", () => {
  const scope: ScopeInfo = {
    path: "/billing",
    title: "Billing",
    section: { kind: "domain", namespace: "default", name: "billing" },
    href: "https://example.com/billing",
  };

  it("returns undefined when scope is undefined", () => {
    expect(toSelfItem(undefined)).toBeUndefined();
  });

  it("returns a NavItem carrying title, path, section, and href", () => {
    expect(toSelfItem(scope)).toEqual({
      title: "Billing",
      path: "/billing",
      section: scope.section,
      href: "https://example.com/billing",
    });
  });
});
