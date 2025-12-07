<script lang="ts">
  import { navigation } from "../stores/navigation";
  import NavTree from "./NavTree.svelte";
</script>

{#if $navigation.loading}
  <div class="text-gray-500 text-sm">Loading...</div>
{:else if $navigation.error}
  <div class="text-red-600 text-sm">{$navigation.error}</div>
{:else if $navigation.tree}
  <nav>
    <NavTree items={$navigation.tree.items} />
  </nav>
{/if}
