<script lang="ts">
  import { path } from "../stores/router";
  import { navigation } from "../stores/navigation";
  import type { NavItem } from "../types";
  import NavTree from "./NavTree.svelte";

  interface Props {
    item: NavItem;
    depth: number;
  }

  let { item, depth }: Props = $props();

  // Check if this item is active (item.path already has leading slash)
  let isActive = $derived($path === item.path);
  let hasChildren = $derived(item.children && item.children.length > 0);
  let isExpanded = $derived(!$navigation.collapsed.has(item.path));

  function toggleExpanded(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    navigation.toggle(item.path);
  }
</script>

<li>
  <div class="flex items-center">
    {#if hasChildren}
      <button
        onclick={toggleExpanded}
        class="w-4 h-4 flex items-center justify-center text-gray-400 hover:text-gray-600 mr-0.5"
        aria-label={isExpanded ? "Collapse" : "Expand"}
      >
        <svg
          class="w-3 h-3 transition-transform {isExpanded ? 'rotate-90' : 'rotate-0'}"
          fill="currentColor"
          viewBox="0 0 20 20"
        >
          <path
            fill-rule="evenodd"
            d="M7.293 14.707a1 1 0 010-1.414L10.586 10 7.293 6.707a1 1 0 011.414-1.414l4 4a1 1 0 010 1.414l-4 4a1 1 0 01-1.414 0z"
            clip-rule="evenodd"
          />
        </svg>
      </button>
    {:else}
      <!-- Spacer matches expand button: w-4 (16px) + mr-0.5 (2px) = 18px -->
      <span class="w-[18px]"></span>
    {/if}

    <a
      href={item.path}
      class="flex-1 py-1 px-1.5 rounded text-sm transition-colors {isActive
        ? 'text-blue-700 font-medium'
        : 'text-gray-700 hover:text-gray-900'}"
    >
      {item.title}
    </a>
  </div>

  {#if hasChildren && isExpanded}
    <NavTree items={item.children!} depth={depth + 1} />
  {/if}
</li>
