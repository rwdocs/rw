<script lang="ts">
  import type { NavGroup } from "../types";
  import NavItemComponent from "./NavItem.svelte";

  interface Props {
    group: NavGroup;
  }

  let { group }: Props = $props();
</script>

{#if group.label}
  <div class="mt-5 first:mt-0">
    <div
      class="px-1.5 pb-1.5 text-xs font-semibold tracking-wider text-gray-500 dark:text-neutral-400 uppercase"
    >
      {group.label}
    </div>
    <ul>
      {#each group.items as item (item.path)}
        <NavItemComponent {item} depth={0} />
      {/each}
    </ul>
  </div>
{:else}
  <ul>
    {#each group.items as item (item.path)}
      <NavItemComponent {item} depth={0} />
    {/each}
  </ul>
{/if}
