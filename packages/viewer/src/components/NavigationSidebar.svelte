<script lang="ts">
  import { getRwContext } from "$lib/context";
  import Alert from "$lib/ui/primitives/Alert.svelte";
  import Chevron from "$lib/ui/primitives/Chevron.svelte";
  import NavTree from "./NavTree.svelte";

  const { navigation, router } = getRwContext();

  let backLink = $derived(navigation.tree?.parentScope ?? null);
</script>

{#if navigation.loading}
  <div class="text-sm text-gray-600 dark:text-neutral-400">Loading...</div>
{:else if navigation.error}
  <Alert intent="danger">{navigation.error}</Alert>
{:else if navigation.tree}
  <nav aria-label="Documentation">
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
            <Chevron direction="left" />
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
