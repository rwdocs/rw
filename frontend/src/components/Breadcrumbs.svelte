<script lang="ts">
  import type { Breadcrumb } from "../types";

  interface Props {
    breadcrumbs: Breadcrumb[];
  }

  let { breadcrumbs }: Props = $props();
</script>

{#if breadcrumbs.length > 0}
  <nav class="mb-6">
    <ol class="flex items-center text-sm text-gray-500">
      <li>
        <a href="/" class="hover:text-gray-700 hover:underline">Home</a>
      </li>
      {#each breadcrumbs as crumb (crumb.path)}
        <li class="flex items-center">
          <svg
            class="w-4 h-4 mx-2 text-gray-400"
            fill="currentColor"
            viewBox="0 0 20 20"
          >
            <path
              fill-rule="evenodd"
              d="M7.293 14.707a1 1 0 010-1.414L10.586 10 7.293 6.707a1 1 0 011.414-1.414l4 4a1 1 0 010 1.414l-4 4a1 1 0 01-1.414 0z"
              clip-rule="evenodd"
            />
          </svg>
          <a
            href="/docs{crumb.path}"
            class="hover:text-gray-700 hover:underline"
          >
            {crumb.title}
          </a>
        </li>
      {/each}
    </ol>
  </nav>
{/if}
