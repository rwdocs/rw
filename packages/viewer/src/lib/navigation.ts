import type { NavItem, NavGroup } from "../types";

/**
 * Pluralize kind names for group labels.
 */
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
 */
export function groupNavItems(items: NavItem[]): NavGroup[] {
  const kindGroups = new Map<string, NavItem[]>();
  const ungrouped: NavItem[] = [];

  for (const item of items) {
    if (item.section) {
      const group = kindGroups.get(item.section.kind) ?? [];
      group.push(item);
      kindGroups.set(item.section.kind, group);
    } else {
      ungrouped.push(item);
    }
  }

  // Build result: ungrouped first, then kind groups (sorted)
  const groups: NavGroup[] = [];

  if (ungrouped.length > 0) {
    groups.push({ label: null, items: ungrouped });
  }

  const kindGroupsSorted = [...kindGroups.entries()]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([kind, groupItems]) => ({
      label: pluralizeKind(kind),
      items: groupItems,
    }));

  groups.push(...kindGroupsSorted);

  return groups;
}
