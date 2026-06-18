<script lang="ts">
  import { getRwContext } from "$lib/context";
  import type { NavItem } from "../types";
  import Button from "$lib/ui/primitives/Button.svelte";
  import Chevron from "$lib/ui/primitives/Chevron.svelte";
  import NavTree from "./NavTree.svelte";

  interface Props {
    item: NavItem;
    depth: number;
  }

  let { item, depth }: Props = $props();

  const { router, navigation } = getRwContext();

  // Check if this item is active (item.path already has leading slash)
  let isActive = $derived(router.path === item.path);
  let hasChildren = $derived(item.children && item.children.length > 0);
  let isExpanded = $derived(!navigation.collapsed.has(item.path));

  function toggleExpanded(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    navigation.toggle(item.path);
  }
</script>

<li>
  <div class="flex items-center">
    {#if hasChildren}
      <Button
        variant="ghost"
        size="xs"
        iconOnly
        onclick={toggleExpanded}
        class="mr-0.5"
        aria-label={isExpanded ? "Collapse" : "Expand"}
      >
        <Chevron direction={isExpanded ? "down" : "right"} class="transition-transform" />
      </Button>
    {:else}
      <!-- Spacer matches expand button: w-5 (20px) + mr-0.5 (2px) = 22px -->
      <span class="w-[22px]"></span>
    {/if}

    <a
      href={item.href ?? router.prefixPath(item.path)}
      class="
        flex-1 rounded-sm p-1.5 text-sm transition-colors
        {isActive
        ? 'font-medium text-blue-700 dark:text-blue-400'
        : `text-gray-700 hover:text-gray-900 dark:text-neutral-300 dark:hover:text-neutral-100`}"
    >
      {item.title}
    </a>
  </div>

  {#if hasChildren && isExpanded}
    <NavTree items={item.children!} depth={depth + 1} />
  {/if}
</li>
