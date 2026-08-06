<script lang="ts">
  import type { NavItem } from "../types";
  import { groupNavItems } from "$lib/navigation";
  import NavItemComponent from "./NavItem.svelte";
  import NavGroup from "./NavGroup.svelte";

  interface Props {
    items: NavItem[];
    depth?: number;
    selfItem?: NavItem;
  }

  let { items, depth = 0, selfItem }: Props = $props();

  // Only group at the top level (depth 0)
  let groups = $derived(depth === 0 ? groupNavItems(items, selfItem) : null);
</script>

{#if depth === 0 && groups}
  {#each groups as group (group.label ?? "ungrouped")}
    <NavGroup {group} />
  {/each}
{:else}
  <ul class={{ "ml-3": depth > 0 }}>
    {#each items as item (item.path)}
      <NavItemComponent {item} {depth} />
    {/each}
  </ul>
{/if}
