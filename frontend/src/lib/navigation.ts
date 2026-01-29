import type { NavItem, NavGroup } from "../types";

/**
 * Pluralize type names for group labels.
 */
function pluralizeType(type: string): string {
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
 * Group navigation items by section type.
 * Returns ungrouped items first, then typed groups (alphabetically).
 */
export function groupNavItems(items: NavItem[]): NavGroup[] {
  const typedGroups = new Map<string, NavItem[]>();
  const ungrouped: NavItem[] = [];

  for (const item of items) {
    if (item.section_type) {
      const group = typedGroups.get(item.section_type) ?? [];
      group.push(item);
      typedGroups.set(item.section_type, group);
    } else {
      ungrouped.push(item);
    }
  }

  // Build result: ungrouped first, then typed groups (sorted)
  const groups: NavGroup[] = [];

  if (ungrouped.length > 0) {
    groups.push({ label: null, items: ungrouped });
  }

  const typedGroupsSorted = [...typedGroups.entries()]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([type, groupItems]) => ({
      label: pluralizeType(type),
      items: groupItems,
    }));

  groups.push(...typedGroupsSorted);

  return groups;
}
