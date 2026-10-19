<script lang="ts">
  import { getRwContext } from "$lib/context";
  import Alert from "$lib/ui/primitives/Alert.svelte";
  import Chevron from "$lib/ui/primitives/Chevron.svelte";
  import { groupNavItems } from "$lib/navigation";
  import NavGroup from "./NavGroup.svelte";

  const { navigation, router } = getRwContext();

  let backLink = $derived(navigation.tree?.parentScope ?? null);
</script>

{#if navigation.loading}
  <div class="text-sm text-neutral-600 dark:text-neutral-400">Loading...</div>
{:else if navigation.error}
  <Alert intent="danger">Failed to load navigation: {navigation.error}</Alert>
{:else if navigation.tree}
  <nav aria-label="Documentation">
    {#if backLink}
      <a
        href={router.hrefFor(backLink)}
        class="
          mb-5 flex items-start text-sm text-fg-muted
          hover:text-blue-600
          dark:hover:text-blue-400
        "
      >
        <!-- The label sizes from its content, so both items shrink in
             proportion and shrink-0 is what holds the gutter. h-5 matches the
             label's line box so the chevron tracks its first line. -->
        <span class="flex h-5 w-[22px] shrink-0 items-center justify-center">
          <Chevron direction="left" />
        </span>
        <span class="px-1.5 wrap-anywhere">{backLink.title}</span>
      </a>
    {/if}
    {#each groupNavItems(navigation.tree.items, navigation.tree.scope) as group (group.label ?? "ungrouped")}
      <NavGroup {group} />
    {/each}
  </nav>
{/if}
