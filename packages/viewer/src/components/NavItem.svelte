<script lang="ts">
  import { getRwContext } from "../lib/context";
  import type { NavItem } from "../types";
  import Button from "../lib/ui/primitives/Button.svelte";
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
        <svg
          class="
            size-3.5 transition-transform
            {isExpanded ? 'rotate-90' : `rotate-0`}"
          fill="currentColor"
          viewBox="0 0 20 20"
        >
          <!-- svelte-ignore component_name_lowercase -->
          <path
            fill-rule="evenodd"
            d="M7.293 14.707a1 1 0 010-1.414L10.586 10 7.293 6.707a1 1 0 011.414-1.414l4 4a1 1 0 010 1.414l-4 4a1 1 0 01-1.414 0z"
            clip-rule="evenodd"
          />
        </svg>
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
