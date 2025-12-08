<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "../stores/router";
  import { navigation } from "../stores/navigation";

  onMount(() => {
    // Redirect to first documentation page if available
    const unsubscribe = navigation.subscribe((state) => {
      if (state.tree && state.tree.items.length > 0) {
        const firstItem = state.tree.items[0];
        goto(`/docs${firstItem.path}`);
      }
    });
    return unsubscribe;
  });
</script>

<div class="flex items-center justify-center h-full">
  <div class="text-center">
    <h1 class="text-3xl font-semibold tracking-tight text-gray-900 mb-4">Docstage</h1>
    <p class="text-gray-600">Where documentation takes the stage</p>
  </div>
</div>
