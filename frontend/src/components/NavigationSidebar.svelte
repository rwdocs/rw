<script lang="ts">
  import { navigation } from "../stores/navigation";
  import NavTree from "./NavTree.svelte";

  let backLink = $derived.by(() => {
    const tree = $navigation.tree;
    if (!tree?.scope) return null;
    return tree.parentScope ?? { path: "/", title: "Home" };
  });
</script>

{#if $navigation.loading}
  <div class="text-gray-600 text-sm">Loading...</div>
{:else if $navigation.error}
  <div class="text-red-600 text-sm">{$navigation.error}</div>
{:else if $navigation.tree}
  <nav>
    {#if $navigation.tree.scope && backLink}
      <div class="mb-5">
        <a
          href={backLink.path}
          class="text-sm text-gray-400 hover:text-gray-600 flex items-center mb-2"
        >
          <span class="w-[22px] flex items-center justify-center">
            <svg class="w-3.5 h-3.5 rotate-180" fill="currentColor" viewBox="0 0 20 20">
              <path
                fill-rule="evenodd"
                d="M7.293 14.707a1 1 0 010-1.414L10.586 10 7.293 6.707a1 1 0 011.414-1.414l4 4a1 1 0 010 1.414l-4 4a1 1 0 01-1.414 0z"
                clip-rule="evenodd"
              />
            </svg>
          </span>
          <span class="px-1.5">{backLink.title}</span>
        </a>
        <h2 class="text-xl font-light text-gray-900 pl-[28px]">{$navigation.tree.scope.title}</h2>
      </div>
    {/if}
    <NavTree items={$navigation.tree.items} />
  </nav>
{/if}
