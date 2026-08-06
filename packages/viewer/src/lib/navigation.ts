import type { NavItem, NavGroup, ScopeInfo } from "../types";

/**
 * Build the scope's own row for the sidebar. `scope.href` is already resolved
 * for embedded mounts by `resolveNavTree`, so this must not fall back to
 * `prefixPath` on its own.
 */
export function toSelfItem(scope: ScopeInfo | undefined): NavItem | undefined {
  if (!scope) return undefined;
  return {
    title: scope.title,
    path: scope.path,
    section: scope.section,
    href: scope.href,
  };
}

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
 *
 * `selfItem` is the scope's own page. It bypasses grouping: every scope root
 * carries a section, so grouping it would file the row under a kind heading
 * below its own children.
 */
export function groupNavItems(items: NavItem[], selfItem?: NavItem): NavGroup[] {
  const kindGroups = new Map<string, NavItem[]>();
  const ungrouped: NavItem[] = selfItem ? [selfItem] : [];

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
