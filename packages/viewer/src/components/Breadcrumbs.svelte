<script lang="ts">
  import { getRwContext } from "../lib/context";
  import { dismissible } from "../lib/dismissible";
  import type { Breadcrumb } from "../types";

  interface Props {
    breadcrumbs: Breadcrumb[];
    compact?: boolean;
  }

  let { breadcrumbs, compact = false }: Props = $props();

  const { router } = getRwContext();

  let navEl: HTMLElement | undefined = $state();
  let olEl: HTMLOListElement | undefined = $state();
  let wrapperEl: HTMLDivElement | undefined = $state();
  let open = $state(false);
  let hiddenCount = $state(0);
  // Plain variables (not $state) — only read inside imperative measurement functions.
  // Measurement is two-pass: first pass computes hiddenCount with ellipsisWidth=0,
  // then a second pass (via $effect) measures the rendered ellipsis and recomputes.
  // The nav's overflow-hidden clips any momentary extra item during the first frame.
  let itemWidths: number[] = [];
  let itemWidthsTotal = 0;
  let ellipsisWidth = 0;
  let lastContainerWidth = 0;
  let rafId = 0;

  let firstCrumb = $derived(breadcrumbs.length > 0 ? breadcrumbs[0] : null);
  let lastCrumb = $derived(breadcrumbs.length > 1 ? breadcrumbs[breadcrumbs.length - 1] : null);
  let hiddenCrumbs = $derived(breadcrumbs.length > 2 ? breadcrumbs.slice(1, 1 + hiddenCount) : []);
  let visibleMiddleCrumbs = $derived(
    breadcrumbs.length > 2 ? breadcrumbs.slice(1 + hiddenCount, breadcrumbs.length - 1) : [],
  );

  function close() {
    open = false;
  }

  function toggle() {
    open = !open;
  }

  $effect(() => dismissible(open, wrapperEl, close));

  function computeHiddenCount(force = false) {
    if (!navEl || itemWidths.length === 0) return;

    const containerWidth = navEl.clientWidth;
    if (!force && containerWidth === lastContainerWidth) return;
    lastContainerWidth = containerWidth;

    if (itemWidthsTotal <= containerWidth) {
      hiddenCount = 0;
      return;
    }

    const middleCount = breadcrumbs.length - 2;
    let runningTotal = itemWidthsTotal;

    for (let i = 0; i < middleCount; i++) {
      const middleIndex = i + 1;
      if (i === 0) {
        runningTotal = runningTotal - itemWidths[middleIndex] + ellipsisWidth;
      } else {
        runningTotal -= itemWidths[middleIndex];
      }

      if (runningTotal <= containerWidth) {
        hiddenCount = i + 1;
        return;
      }
    }

    hiddenCount = middleCount;
  }

  function measureItemWidths() {
    if (!navEl || !olEl || breadcrumbs.length <= 2) {
      itemWidths = [];
      itemWidthsTotal = 0;
      hiddenCount = 0;
      return;
    }

    hiddenCount = 0;
    ellipsisWidth = 0;
    lastContainerWidth = 0;

    cancelAnimationFrame(rafId);
    rafId = requestAnimationFrame(() => {
      if (!olEl) return;
      const children = olEl.children;
      itemWidths = [];
      itemWidthsTotal = 0;
      for (let i = 0; i < children.length; i++) {
        const w = (children[i] as HTMLElement).offsetWidth;
        itemWidths.push(w);
        itemWidthsTotal += w;
      }
      computeHiddenCount(true);
    });
  }

  function measureEllipsis() {
    if (!olEl) return;
    const ellipsisLi = olEl.children[1] as HTMLElement | undefined;
    if (!ellipsisLi) return;
    ellipsisWidth = ellipsisLi.offsetWidth;
    lastContainerWidth = 0;
    computeHiddenCount(true);
  }

  $effect(() => {
    if (hiddenCount > 0 && ellipsisWidth === 0) {
      requestAnimationFrame(measureEllipsis);
    }
  });

  $effect(() => {
    void breadcrumbs;
    close();
    measureItemWidths();
  });

  $effect(() => {
    if (!navEl) return;

    const observer = new ResizeObserver(() => {
      // Re-measure item widths when transitioning from hidden (display:none)
      // to visible — all widths will be zero from the initial measurement.
      if (itemWidthsTotal === 0 && breadcrumbs.length > 2) {
        measureItemWidths();
      } else {
        computeHiddenCount();
      }
    });

    observer.observe(navEl);
    return () => observer.disconnect();
  });
</script>

<div class={compact ? "relative min-h-8" : "relative mb-6 min-h-8"} bind:this={wrapperEl}>
  <nav aria-label="Breadcrumb" class="min-h-8 overflow-hidden" bind:this={navEl}>
    {#if breadcrumbs.length > 0}
      <ol
        class="
          flex min-h-8 min-w-fit items-center text-sm whitespace-nowrap text-gray-600
          dark:text-neutral-400
        "
        bind:this={olEl}
      >
        {#if firstCrumb}
          <li
            class={breadcrumbs.length > 1
              ? "after:mx-2 after:text-gray-400 after:content-['/'] dark:after:text-neutral-500"
              : ""}
          >
            <a
              href={router.prefixPath(firstCrumb.path)}
              class="hover:text-gray-700 hover:underline dark:hover:text-neutral-300"
            >
              {firstCrumb.title}
            </a>
          </li>
        {/if}

        {#if hiddenCount > 0}
          <li
            class="after:mx-2 after:text-gray-400 after:content-['/'] dark:after:text-neutral-500"
          >
            <button
              onclick={toggle}
              class="
                cursor-pointer
                hover:text-gray-700 hover:underline
                dark:hover:text-neutral-300
              "
              aria-label="Show hidden breadcrumbs"
              aria-expanded={open}
            >
              &hellip;
            </button>
          </li>
        {/if}

        {#each visibleMiddleCrumbs as crumb (crumb.path)}
          <li
            class="after:mx-2 after:text-gray-400 after:content-['/'] dark:after:text-neutral-500"
          >
            <a
              href={router.prefixPath(crumb.path)}
              class="hover:text-gray-700 hover:underline dark:hover:text-neutral-300"
            >
              {crumb.title}
            </a>
          </li>
        {/each}

        {#if lastCrumb}
          <li>
            <a
              href={router.prefixPath(lastCrumb.path)}
              class="hover:text-gray-700 hover:underline dark:hover:text-neutral-300"
            >
              {lastCrumb.title}
            </a>
          </li>
        {/if}
      </ol>
    {:else}
      <div class="h-8"></div>
    {/if}
  </nav>

  {#if open && hiddenCount > 0}
    <ul
      role="menu"
      aria-label="Hidden breadcrumbs"
      class="
        absolute top-full left-0 z-40 mt-1 min-w-48 overflow-y-auto rounded-lg border
        border-gray-200 bg-white py-1 shadow-lg
        dark:border-neutral-600 dark:bg-neutral-800
      "
    >
      {#each hiddenCrumbs as crumb (crumb.path)}
        <li role="none">
          <a
            href={router.prefixPath(crumb.path)}
            role="menuitem"
            class="
              block px-3 py-1.5 text-sm text-gray-600
              hover:bg-gray-100 hover:text-gray-700
              dark:text-neutral-400
              dark:hover:bg-neutral-700 dark:hover:text-neutral-300
            "
            onclick={close}
          >
            {crumb.title}
          </a>
        </li>
      {/each}
    </ul>
  {/if}
</div>
