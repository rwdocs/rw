import type { NavItem, NavGroup } from "../types";

/**
 * Pluralize kind names for group labels.
 */
function pluralizeKind(type: string): string {
  const map: Record<string, string> = {
    domain: "Domains",
    system: "Systems",
    service: "Services",
    api: "APIs",
    guide: "Guides",
  };
  return map[type.toLowerCase()] ?? `${type}s`;
}

/**
 * Group navigation items by section kind.
 * Returns ungrouped items first, then kind groups (alphabetically).
 */
export function groupNavItems(items: NavItem[]): NavGroup[] {
  const typedGroups = new Map<string, NavItem[]>();
  const ungrouped: NavItem[] = [];

  for (const item of items) {
    if (item.sectionKind) {
      const group = typedGroups.get(item.sectionKind) ?? [];
      group.push(item);
      typedGroups.set(item.sectionKind, group);
    } else {
      ungrouped.push(item);
    }
  }

  // Build result: ungrouped first, then kind groups (sorted)
  const groups: NavGroup[] = [];

  if (ungrouped.length > 0) {
    groups.push({ label: null, items: ungrouped });
  }

  const typedGroupsSorted = [...typedGroups.entries()]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([type, groupItems]) => ({
      label: pluralizeKind(type),
      items: groupItems,
    }));

  groups.push(...typedGroupsSorted);

  return groups;
}
