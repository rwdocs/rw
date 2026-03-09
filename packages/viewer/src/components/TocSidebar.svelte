<script lang="ts">
  import { getRwContext } from "../lib/context";
  import type { TocEntry } from "../types";

  interface Props {
    toc: TocEntry[];
    onnavigate?: () => void;
  }

  let { toc, onnavigate }: Props = $props();

  const { router } = getRwContext();

  // Filter to only show h2 and h3 (two levels deep, excludes h1)
  let filteredToc = $derived(toc.filter((entry) => entry.level >= 2 && entry.level <= 3));

  let activeId = $state<string | null>(null);
  let isUserScrolling = false;

  /** Mark scroll in progress and reset after the browser finishes scrolling. */
  function waitForScrollEnd() {
    isUserScrolling = true;
    const scrollBefore = window.scrollY;
    requestAnimationFrame(() => {
      if (window.scrollY === scrollBefore) {
        isUserScrolling = false;
      } else {
        const fallback = setTimeout(() => {
          isUserScrolling = false;
        }, 500);
        window.addEventListener(
          "scrollend",
          () => {
            clearTimeout(fallback);
            isUserScrolling = false;
          },
          { once: true },
        );
      }
    });
  }

  // React to hash changes (e.g., when page loads with #hash or when clicking links)
  $effect(() => {
    const currentHash = router.hash;
    if (currentHash && filteredToc.some((entry) => entry.id === currentHash)) {
      activeId = currentHash;

      if (!router.embedded) {
        waitForScrollEnd();
      }
    }
  });

  function scrollToHeading(event: MouseEvent, id: string) {
    event.preventDefault();
    const element = document.getElementById(id);
    if (element) {
      activeId = id;
      element.scrollIntoView({ behavior: "auto" });
      history.pushState(null, "", `#${id}`);
      waitForScrollEnd();
    }
  }

  $effect(() => {
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

    // Set initial active heading to first one (only if no hash-based selection)
    if (filteredToc.length > 0 && !activeId) {
      activeId = filteredToc[0].id;
    }

    // In embedded mode, the router skips popstate handling. Listen here so
    // browser Back/Forward after TOC clicks scrolls to the correct heading.
    const handlePopState = router.embedded
      ? () => {
          const id = decodeURIComponent(window.location.hash.slice(1));
          if (id && filteredToc.some((entry) => entry.id === id)) {
            activeId = id;
            const element = document.getElementById(id);
            if (element) {
              element.scrollIntoView({ behavior: "auto" });
            }
          }
        }
      : null;
    if (handlePopState) {
      window.addEventListener("popstate", handlePopState);
    }

    return () => {
      observer.disconnect();
      if (handlePopState) {
        window.removeEventListener("popstate", handlePopState);
      }
    };
  });
</script>

<div>
  <h3
    class="mb-3 text-xs font-semibold tracking-wider text-gray-600 uppercase dark:text-neutral-400"
  >
    On this page
  </h3>
  <ul class="space-y-1.5">
    {#each filteredToc as entry (entry.id)}
      <li class={entry.level === 3 ? "ml-3" : ""}>
        <a
          href="#{entry.id}"
          onclick={(e) => {
            scrollToHeading(e, entry.id);
            onnavigate?.();
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
