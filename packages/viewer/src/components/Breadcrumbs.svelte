<script lang="ts">
  import { Menu } from "$lib/ui/primitives/Menu";
  import { useElementSize } from "$lib/ui/hooks/useElementSize.svelte";
  import { getRwContext } from "$lib/context";
  import type { Breadcrumb } from "../types";

  interface Props {
    breadcrumbs: Breadcrumb[];
    compact?: boolean;
  }

  let { breadcrumbs, compact = false }: Props = $props();

  const { router } = getRwContext();

  let navEl: HTMLElement | undefined = $state();
  let olEl: HTMLOListElement | undefined = $state();
  let ellipsisEl: HTMLButtonElement | undefined = $state();
  let open = $state(false);
  let hiddenCount = $state(0);
  const navSize = useElementSize(() => navEl ?? null);
  // Plain variables (not $state) — only read inside imperative measurement functions.
  // Measurement is two-pass: first pass computes hiddenCount with ellipsisWidth=0,
  // then a second pass (via $effect) measures the rendered ellipsis and recomputes.
  // The nav's overflow-hidden clips any momentary extra item during the first frame.
  let itemWidths: number[] = [];
  let itemWidthsTotal = 0;
  let ellipsisWidth = 0;
  let lastContainerWidth = 0;
  let rafId = 0;

  let lastCrumb = $derived(breadcrumbs.length > 1 ? breadcrumbs[breadcrumbs.length - 1] : null);
  let hiddenCrumbs = $derived(breadcrumbs.length > 2 ? breadcrumbs.slice(1, 1 + hiddenCount) : []);
  let visibleMiddleCrumbs = $derived(
    breadcrumbs.length > 2 ? breadcrumbs.slice(1 + hiddenCount, breadcrumbs.length - 1) : [],
  );

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
    open = false;
    measureItemWidths();
  });

  $effect(() => {
    void navSize.version;
    // Re-measure item widths when transitioning from hidden (display:none)
    // to visible — all widths will be zero from the initial measurement.
    if (itemWidthsTotal === 0 && breadcrumbs.length > 2) {
      measureItemWidths();
    } else {
      computeHiddenCount();
    }
  });
</script>

{#snippet crumb(c: Breadcrumb)}
  <li>
    <a
      href={router.hrefFor(c)}
      class="hover:text-gray-700 hover:underline dark:hover:text-neutral-300"
    >
      {c.title}
    </a>
  </li>
{/snippet}

<div class={["relative", { "mb-6": !compact }]}>
  <nav aria-label="Breadcrumb" class="min-h-8 overflow-hidden" bind:this={navEl}>
    {#if breadcrumbs.length > 0}
      <!-- measureEllipsis reads children[1]: keep the ellipsis li second. -->
      <ol
        class="
          flex min-h-8 min-w-fit items-center text-sm whitespace-nowrap text-gray-600
          *:not-last:after:mx-2 *:not-last:after:text-gray-400 *:not-last:after:content-['/']
          dark:text-neutral-400
          dark:*:not-last:after:text-neutral-500
        "
        bind:this={olEl}
      >
        {@render crumb(breadcrumbs[0])}

        {#if hiddenCount > 0}
          <li>
            <button
              type="button"
              bind:this={ellipsisEl}
              onclick={() => (open = !open)}
              class="
                cursor-pointer
                hover:text-gray-700 hover:underline
                dark:hover:text-neutral-300
              "
              aria-label="Show hidden breadcrumbs"
            >
              &hellip;
            </button>
          </li>
        {/if}

        {#each visibleMiddleCrumbs as c (c.path)}
          {@render crumb(c)}
        {/each}

        {#if lastCrumb}
          {@render crumb(lastCrumb)}
        {/if}
      </ol>
    {/if}
  </nav>

  {#if hiddenCount > 0}
    <Menu.Root bind:open anchorEl={ellipsisEl ?? null} aria-label="Hidden breadcrumbs">
      {#each hiddenCrumbs as c (c.path)}
        <Menu.Item href={router.hrefFor(c)}>
          {c.title}
        </Menu.Item>
      {/each}
    </Menu.Root>
  {/if}
</div>
