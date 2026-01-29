<script lang="ts">
  import { navigation } from "../stores/navigation";
  import NavTree from "./NavTree.svelte";
</script>

{#if $navigation.loading}
  <div class="text-gray-600 text-sm">Loading...</div>
{:else if $navigation.error}
  <div class="text-red-600 text-sm">{$navigation.error}</div>
{:else if $navigation.tree}
  <nav>
    {#if $navigation.tree.scope}
      <div class="mb-4 pb-3 border-b border-gray-200">
        {#if $navigation.tree.parentScope}
          <a
            href={$navigation.tree.parentScope.path}
            class="text-sm text-gray-500 hover:text-gray-700 flex items-center gap-1 mb-1"
          >
            <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M15 19l-7-7 7-7"
              />
            </svg>
            Back to {$navigation.tree.parentScope.title}
          </a>
        {:else}
          <a
            href="/"
            class="text-sm text-gray-500 hover:text-gray-700 flex items-center gap-1 mb-1"
          >
            <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M15 19l-7-7 7-7"
              />
            </svg>
            Back to Home
          </a>
        {/if}
        <h2 class="text-lg font-semibold text-gray-900">{$navigation.tree.scope.title}</h2>
      </div>
    {/if}
    <NavTree items={$navigation.tree.items} />
  </nav>
{/if}
