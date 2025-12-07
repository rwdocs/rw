<script lang="ts">
  import type { TocEntry } from "../types";

  interface Props {
    toc: TocEntry[];
  }

  let { toc }: Props = $props();

  function scrollToHeading(id: string) {
    const element = document.getElementById(id);
    if (element) {
      element.scrollIntoView({ behavior: "smooth" });
    }
  }
</script>

<div>
  <h3 class="text-xs font-semibold text-gray-500 uppercase tracking-wider mb-3">
    On this page
  </h3>
  <ul class="space-y-2">
    {#each toc as entry (entry.id)}
      <li style="margin-left: {(entry.level - 2) * 12}px">
        <button
          onclick={() => scrollToHeading(entry.id)}
          class="text-sm text-gray-600 hover:text-gray-900 text-left transition-colors"
        >
          {entry.title}
        </button>
      </li>
    {/each}
  </ul>
</div>
