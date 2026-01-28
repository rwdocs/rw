<script lang="ts">
  import { onMount } from "svelte";
  import type { TocEntry } from "../types";

  interface Props {
    toc: TocEntry[];
  }

  let { toc }: Props = $props();

  // Filter to only show h2 and h3 (two levels deep, excludes h1)
  let filteredToc = $derived(toc.filter((entry) => entry.level >= 2 && entry.level <= 3));

  let activeId = $state<string | null>(null);
  let isUserScrolling = false;

  function scrollToHeading(id: string) {
    const element = document.getElementById(id);
    if (element) {
      activeId = id;
      isUserScrolling = true;
      element.scrollIntoView({ behavior: "smooth" });
      // Re-enable observer updates after scroll animation completes
      setTimeout(() => {
        isUserScrolling = false;
      }, 1000);
    }
  }

  onMount(() => {
    if (filteredToc.length === 0) return;

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

        // Skip updates during programmatic scrolling
        if (isUserScrolling) {
          return;
        }

        // Find the topmost visible heading based on ToC order
        for (const tocEntry of filteredToc) {
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
    for (const entry of filteredToc) {
      const element = document.getElementById(entry.id);
      if (element) {
        observer.observe(element);
      }
    }

    // Set initial active heading to first one
    if (filteredToc.length > 0) {
      activeId = filteredToc[0].id;
    }

    return () => {
      observer.disconnect();
    };
  });
</script>

<div>
  <h3 class="text-xs font-semibold text-gray-600 uppercase tracking-wider mb-3">On this page</h3>
  <ul class="space-y-1.5">
    {#each filteredToc as entry (entry.id)}
      <li class={entry.level === 3 ? "ml-3" : ""}>
        <button
          onclick={() => scrollToHeading(entry.id)}
          class="text-sm leading-snug text-left transition-colors {activeId === entry.id
            ? 'text-blue-600 font-medium'
            : 'text-gray-600 hover:text-gray-900'}"
        >
          {entry.title}
        </button>
      </li>
    {/each}
  </ul>
</div>
