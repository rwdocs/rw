<script lang="ts">
  import type { TocEntry } from "../types";

  interface Props {
    toc: TocEntry[];
    activeId: string | null;
    onNavigate: (id: string) => void;
  }

  let { toc, activeId, onNavigate }: Props = $props();
</script>

<div>
  <h3
    class="mb-3 text-xs font-semibold tracking-wider text-gray-600 uppercase dark:text-neutral-400"
  >
    On this page
  </h3>
  <ul class="space-y-1.5">
    {#each toc as entry (entry.id)}
      <li class={entry.level === 3 ? "ml-3" : ""}>
        <a
          href="#{entry.id}"
          onclick={(e) => {
            e.preventDefault();
            onNavigate(entry.id);
          }}
          class="
            block text-sm/snug transition-colors
            {activeId === entry.id
            ? 'font-medium text-blue-600 dark:text-blue-400'
            : `text-gray-600 hover:text-gray-900 dark:text-neutral-400 dark:hover:text-neutral-100`}"
        >
          {entry.title}
        </a>
      </li>
    {/each}
  </ul>
</div>
