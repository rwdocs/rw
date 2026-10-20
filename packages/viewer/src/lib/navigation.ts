import type { NavItem, NavGroup } from "../types";

function pluralizeKind(kind: string): string {
  const map: Record<string, string> = {
    domain: "Domains",
    system: "Systems",
    service: "Services",
    api: "APIs",
    guide: "Guides",
  };
  return map[kind.toLowerCase()] ?? `${kind}s`;
}

/**
 * Group navigation items by section kind.
 * Returns ungrouped items first, then kind groups (alphabetically).
 *
 * `selfItem` is the scope's own page. It bypasses grouping: every scope root
 * carries a section, so grouping it would file the row under a kind heading
 * below its own children.
 */
export function groupNavItems(items: NavItem[], selfItem: NavItem): NavGroup[] {
  const kindGroups = new Map<string, NavItem[]>();
  const ungrouped: NavItem[] = [selfItem];

  for (const item of items) {
    if (item.section) {
      const group = kindGroups.get(item.section.kind) ?? [];
      group.push(item);
      kindGroups.set(item.section.kind, group);
    } else {
      ungrouped.push(item);
    }
  }

  const kindGroupsSorted = [...kindGroups.entries()]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([kind, groupItems]) => ({
      label: pluralizeKind(kind),
      items: groupItems,
    }));

  return [{ label: null, items: ungrouped }, ...kindGroupsSorted];
}
