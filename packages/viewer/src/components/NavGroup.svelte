<script lang="ts">
  import type { NavGroup } from "../types";
  import NavItemComponent from "./NavItem.svelte";

  interface Props {
    group: NavGroup;
  }

  let { group }: Props = $props();
</script>

{#snippet list()}
  <ul>
    {#each group.items as item (item.path)}
      <NavItemComponent {item} depth={0} />
    {/each}
  </ul>
{/snippet}

{#if group.label}
  <div class="mt-5 first:mt-0">
    <div
      class="
        px-1.5 pb-1.5 text-xs font-semibold tracking-wider wrap-break-word text-fg-muted uppercase
      "
    >
      {group.label}
    </div>
    {@render list()}
  </div>
{:else}
  {@render list()}
{/if}
