<script lang="ts">
  import { getRwContext } from "../lib/context";
  import Alert from "../lib/ui/primitives/Alert.svelte";
  import NavTree from "./NavTree.svelte";

  const { navigation, router } = getRwContext();

  let backLink = $derived(navigation.tree?.parentScope ?? null);
</script>

{#if navigation.loading}
  <div class="text-sm text-gray-600 dark:text-neutral-400">Loading...</div>
{:else if navigation.error}
  <Alert intent="danger">{navigation.error}</Alert>
{:else if navigation.tree}
  <nav>
    {#if navigation.tree.scope && backLink}
      <div class="mb-5">
        <a
          href={backLink.href ?? router.prefixPath(backLink.path)}
          class="
            mb-2 flex items-center text-sm text-gray-500
            hover:text-blue-600
            dark:text-neutral-400
            dark:hover:text-blue-400
          "
        >
          <span class="flex w-[22px] items-center justify-center">
            <svg class="size-3.5 rotate-180" fill="currentColor" viewBox="0 0 20 20">
              <path
                fill-rule="evenodd"
                d="M7.293 14.707a1 1 0 010-1.414L10.586 10 7.293 6.707a1 1 0 011.414-1.414l4 4a1 1 0 010 1.414l-4 4a1 1 0 01-1.414 0z"
                clip-rule="evenodd"
              />
            </svg>
          </span>
          <span class="px-1.5">{backLink.title}</span>
        </a>
        <h2 class="pl-[28px] text-xl font-light text-gray-900 dark:text-neutral-100">
          {navigation.tree.scope.title}
        </h2>
      </div>
    {/if}
    <NavTree items={navigation.tree.items} />
  </nav>
{/if}
