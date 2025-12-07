<script lang="ts">
  import type { NavItem } from "../types";
  import NavItemComponent from "./NavItem.svelte";

  interface Props {
    items: NavItem[];
    depth?: number;
  }

  let { items, depth = 0 }: Props = $props();
</script>

<ul class={depth > 0 ? "ml-4" : ""}>
  {#each items as item (item.path)}
    <NavItemComponent {item} {depth} />
  {/each}
</ul>
