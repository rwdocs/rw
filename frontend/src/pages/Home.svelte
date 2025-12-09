<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "../stores/router";

  onMount(() => {
    // Redirect to root documentation page
    goto("/index");
  });
</script>

<div class="flex items-center justify-center h-full">
  <div class="text-center">
    <h1 class="text-3xl font-semibold tracking-tight text-gray-900 mb-4">Docstage</h1>
    <p class="text-gray-600">Where documentation takes the stage</p>
  </div>
</div>
