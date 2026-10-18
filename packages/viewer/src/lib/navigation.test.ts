import { describe, it, expect } from "vitest";
import { groupNavItems } from "./navigation";
import type { NavItem } from "../types";

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
    expect(groups).toHaveLength(2);
  });

  it("is unchanged when no self item is given", () => {
    const groups = groupNavItems([plain, domain]);

    expect(groups[0].label).toBeNull();
    expect(groups[0].items).toEqual([plain]);
    expect(groups[1].label).toBe("Domains");
  });
});
