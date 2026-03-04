<script lang="ts">
  import { getRwContext } from "../lib/context";
  import type { Breadcrumb } from "../types";

  interface Props {
    breadcrumbs: Breadcrumb[];
  }

  let { breadcrumbs }: Props = $props();

  const { router } = getRwContext();
</script>

{#if breadcrumbs.length > 0}
  <nav class="mb-6">
    <ol class="flex items-center text-sm text-gray-600 dark:text-neutral-400">
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
  </nav>
{/if}
