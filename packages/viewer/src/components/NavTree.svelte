<script lang="ts">
  import type { NavItem } from "../types";
  import { groupNavItems } from "../lib/navigation";
  import NavItemComponent from "./NavItem.svelte";
  import NavGroup from "./NavGroup.svelte";

  interface Props {
    items: NavItem[];
    depth?: number;
  }

  let { items, depth = 0 }: Props = $props();

  // Only group at the top level (depth 0)
  let groups = $derived(depth === 0 ? groupNavItems(items) : null);
</script>

{#if depth === 0 && groups}
  {#each groups as group (group.label ?? "ungrouped")}
    <NavGroup {group} />
  {/each}
{:else}
  <ul class={depth > 0 ? "ml-3" : ""}>
    {#each items as item (item.path)}
      <NavItemComponent {item} {depth} />
    {/each}
  </ul>
{/if}
