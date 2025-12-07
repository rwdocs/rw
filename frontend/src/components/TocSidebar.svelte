<script lang="ts">
  import { onMount } from "svelte";
  import type { TocEntry } from "../types";

  interface Props {
    toc: TocEntry[];
  }

  let { toc }: Props = $props();

  let activeId = $state<string | null>(null);

  function scrollToHeading(id: string) {
    const element = document.getElementById(id);
    if (element) {
      element.scrollIntoView({ behavior: "smooth" });
    }
  }

  onMount(() => {
    if (toc.length === 0) return;

    // Track visible headings
    const visibleHeadings = new Set<string>();

    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            visibleHeadings.add(entry.target.id);
          } else {
            visibleHeadings.delete(entry.target.id);
          }
        }

        // Find the topmost visible heading based on ToC order
        for (const tocEntry of toc) {
          if (visibleHeadings.has(tocEntry.id)) {
            activeId = tocEntry.id;
            return;
          }
        }

        // If no headings are visible, keep the last active one
      },
      {
        // Trigger when heading enters the top 20% of viewport
        rootMargin: "-10% 0px -80% 0px",
        threshold: 0,
      },
    );

    // Observe all headings from ToC
    for (const entry of toc) {
      const element = document.getElementById(entry.id);
      if (element) {
        observer.observe(element);
      }
    }

    // Set initial active heading to first one
    if (toc.length > 0) {
      activeId = toc[0].id;
    }

    return () => {
      observer.disconnect();
    };
  });
</script>

<div>
  <h3 class="text-xs font-semibold text-gray-500 uppercase tracking-wider mb-3">
    On this page
  </h3>
  <ul class="space-y-2">
    {#each toc as entry (entry.id)}
      <li
        style="margin-left: {Math.max(
          0,
          Math.min((entry.level - 2) * 12, 48),
        )}px"
      >
        <button
          onclick={() => scrollToHeading(entry.id)}
          class="text-sm text-left transition-colors {activeId === entry.id
            ? 'text-blue-600 font-medium'
            : 'text-gray-600 hover:text-gray-900'}"
        >
          {entry.title}
        </button>
      </li>
    {/each}
  </ul>
</div>
