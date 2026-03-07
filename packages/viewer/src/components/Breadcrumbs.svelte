<script lang="ts">
  import { getRwContext } from "../lib/context";
  import type { Breadcrumb } from "../types";

  interface Props {
    breadcrumbs: Breadcrumb[];
  }

  let { breadcrumbs }: Props = $props();

  const { router } = getRwContext();
</script>

<nav class="mb-6 min-h-8">
  {#if breadcrumbs.length > 0}
    <ol class="flex min-h-8 flex-wrap items-center text-sm text-gray-600 dark:text-neutral-400">
      {#each breadcrumbs as crumb (crumb.path)}
        <li
          class="
            after:mx-2 after:text-gray-400 after:content-['/']
            last:after:content-none
            dark:after:text-neutral-500
          "
        >
          <a
            href={router.prefixPath(crumb.path)}
            class="hover:text-gray-700 hover:underline dark:hover:text-neutral-300"
          >
            {crumb.title}
          </a>
        </li>
      {/each}
    </ol>
  {:else}
    <div class="h-8"></div>
  {/if}
</nav>
