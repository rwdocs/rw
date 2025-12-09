<script lang="ts">
  import type { Breadcrumb } from "../types";

  interface Props {
    breadcrumbs: Breadcrumb[];
  }

  let { breadcrumbs }: Props = $props();
</script>

{#if breadcrumbs.length > 0}
  <nav class="mb-6">
    <ol class="flex items-center text-sm text-gray-600">
      {#each breadcrumbs as crumb (crumb.path)}
        <li class="breadcrumb-item">
          <a
            href={crumb.path}
            class="hover:text-gray-700 hover:underline"
          >
            {crumb.title}
          </a>
        </li>
      {/each}
    </ol>
  </nav>
{/if}

<style>
  .breadcrumb-item::after {
    content: "/";
    margin-left: 0.625rem;
    margin-right: 0.5rem;
    color: rgb(156 163 175); /* gray-400 */
  }

  .breadcrumb-item:last-child::after {
    content: none;
  }
</style>
