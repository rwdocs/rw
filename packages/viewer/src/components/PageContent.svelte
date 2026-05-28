<script lang="ts">
  import { getRwContext } from "$lib/context";
  import { initializeTabs } from "$lib/tabs";
  import { rewriteSectionRefLinks } from "$lib/sectionRefs";
  import { rangeToSelectors, selectorsToRange, type AnchorStrategy } from "$lib/anchoring";
  import { wrapRange, unwrapAll } from "$lib/comments/highlight";
  import LoadingSkeleton from "$lib/ui/primitives/LoadingSkeleton.svelte";
  import Alert from "$lib/ui/primitives/Alert.svelte";
  import Button from "$lib/ui/primitives/Button.svelte";
  import Popover from "$lib/ui/primitives/Popover.svelte";
  import { useElementSize } from "$lib/ui/hooks/useElementSize.svelte";
  import { useSelectionPopover } from "$lib/ui/hooks/useSelectionPopover.svelte";
  import PageComments from "./comments/PageComments.svelte";

  const ctx = getRwContext();
  const { page, router, comments } = ctx;

  let articleRef: HTMLElement | undefined = $state();
  let showSkeleton = $state(false);

  const articleSize = useElementSize(() => articleRef ?? null);

  // Text-selection state for the Add-comment popover. The hook owns the
  // captured Range, the article-relative anchor point, and dismiss-on-collapse.
  const selectionPopover = useSelectionPopover(() => articleRef ?? null, articleSize);

  // Drop any in-flight selection when the article content changes (live reload,
  // navigation). The cached Range's start/end nodes get detached, so its rect
  // collapses to zero and would briefly jump the popover to the top-left
  // corner before anything else dismissed it.
  $effect(() => {
    void page.data;
    selectionPopover.clear();
  });

  // Delay skeleton appearance so fast page loads don't flash it.
  const SKELETON_DELAY_MS = 300;
  $effect(() => {
    if (page.loading) {
      const timeout = setTimeout(() => {
        showSkeleton = true;
      }, SKELETON_DELAY_MS);
      return () => clearTimeout(timeout);
    } else {
      showSkeleton = false;
    }
  });

  // Initialize tabs when content changes
  $effect(() => {
    if (page.data && articleRef) {
      return initializeTabs(articleRef);
    }
  });

  // Rewrite section ref links when content changes (embedded mode with resolver).
  // If the user navigates away during the async resolver call, Svelte replaces
  // the DOM element, so stale writes land on a detached node and are harmless.
  $effect(() => {
    if (!page.data || !articleRef || !ctx.resolveSectionRefs) return;
    rewriteSectionRefLinks(articleRef, ctx.resolveSectionRefs, () => router.getBasePath()).catch(
      (e) => {
        if (import.meta.env.DEV) {
          console.warn("[PageContent] Failed to rewrite section ref links:", e);
        }
      },
    );
  });

  // Scroll to hash target when content loads or hash changes
  $effect(() => {
    const currentHash = router.hash;
    if (page.data && articleRef && currentHash) {
      const target = document.getElementById(currentHash);
      if (target) {
        // Use requestAnimationFrame to ensure DOM is fully rendered
        requestAnimationFrame(() => {
          target.scrollIntoView({ behavior: "auto" });
        });
      }
    }
  });

  // Load comments when page data changes or comments become enabled.
  // On initial load, config may not have arrived yet — reading
  // comments.enabled ensures the effect re-runs when it flips to true.
  $effect(() => {
    if (page.data && comments.enabled) {
      const docId = page.data.meta.path.replace(/^\//, "");
      comments.load(docId);
    } else if (!page.data) {
      comments.clear();
    }
  });

  // Map from comment ID to its anchored Range (for click detection)
  let commentRanges = $state.raw<Map<string, Range>>(new Map());

  // Apply comment highlights via <rw-annotation> DOM wrappers.
  // Overlapping comments nest, so the box-model alpha compositing makes the
  // overlap region a darker yellow — what the CSS Custom Highlight API
  // can't do because it picks one color per overlapping range
  // ("last write wins" across Highlight objects).
  //
  // The active comment uses CSS.highlights.rw-comment-active (next effect)
  // because a single-range overlay doesn't need DOM mutation and shouldn't
  // fight with the user's text selection.
  $effect(() => {
    const items = comments.items;
    const container = articleRef;
    if (!container) return;

    // Drop any pending text selection — its Range may point into nodes we're
    // about to unwrap, which would collapse the selection mid-draft. The
    // popover follows live selection state, so this also dismisses it.
    selectionPopover.clear();
    // Also drop the native selection — its Range may point into text nodes
    // unwrapAll is about to mutate, and a stray mouseup after the wrap pass
    // would read a Range whose containers have been merged.
    window.getSelection()?.removeAllRanges();
    unwrapAll(container);

    const rangeMap = new Map<string, Range>();
    const strategyMap = new Map<string, AnchorStrategy>();
    const orphanIds = new Set<string>();
    const anchored: { id: string; range: Range }[] = [];

    for (const comment of items) {
      if (comment.selectors.length === 0) continue;
      if (comment.status === "resolved") continue;
      const result = selectorsToRange(comment.selectors, container);
      if (result) {
        const wrappers = wrapRange(result.range, {
          commentId: comment.id,
          strategy: result.strategy,
        });
        if (wrappers.length === 0) {
          // Range resolved but contained only whitespace — treat as orphan so
          // the comment surfaces in the page-comments timeline instead of
          // becoming an invisible clickable region (whitespace highlight has
          // no visible wrapper but range.getClientRects() still has bounds).
          if (!comment.parentId) orphanIds.add(comment.id);
          continue;
        }
        rangeMap.set(comment.id, result.range);
        strategyMap.set(comment.id, result.strategy);
        anchored.push({ id: comment.id, range: result.range });
      } else if (!comment.parentId) {
        // Top-level inline comment whose stored selectors no longer resolve —
        // surface it in the page comments timeline below the article instead
        // of silently dropping it. Replies are kept with whichever parent they
        // belong to, so only top-level threads get promoted.
        orphanIds.add(comment.id);
      }
    }

    commentRanges = rangeMap;
    comments.anchorStrategies = strategyMap;
    comments.orphanIds = orphanIds;

    // If the active thread just lost its anchor (e.g. live-reload rewrote the
    // page and the passage is gone), drop activeId — the sidebar filters out
    // orphans, so activeThread would resolve to undefined and leave the panel
    // open but empty. Clearing sends focus back to the main view; the orphan
    // is still reachable from the page comments timeline.
    if (comments.activeId && orphanIds.has(comments.activeId)) {
      comments.activeId = null;
    }

    // Order inline threads by their live DOM position, not by stored
    // TextPositionSelector.start — stored positions reflect the document
    // as it was at comment creation time and are stale after edits.
    anchored.sort((a, b) => a.range.compareBoundaryPoints(Range.START_TO_START, b.range));
    comments.order = anchored.map((a) => a.id);

    return () => {
      unwrapAll(container);
    };
  });

  $effect(() => {
    const activeId = comments.activeId;
    if (!activeId || !articleRef) {
      comments.activeTop = null;
      return;
    }
    void articleSize.version;
    comments.activeTop = getHighlightTop(activeId);
  });

  // Apply active comment highlight (existing comment or pending new comment)
  $effect(() => {
    const activeId = comments.activeId;
    const pending = comments.pending;
    const items = comments.items;
    const container = articleRef;
    if (!container || typeof CSS === "undefined" || !("highlights" in CSS)) return;

    const highlights = CSS.highlights as Map<string, Highlight>;

    // Pending new comment — highlight the selected text
    if (pending && pending.selectors.length > 0) {
      const result = selectorsToRange(pending.selectors, container);
      if (result) {
        highlights.set("rw-comment-active", new Highlight(result.range));
      } else {
        highlights.delete("rw-comment-active");
      }
      return () => {
        highlights.delete("rw-comment-active");
      };
    }

    if (!activeId) {
      highlights.delete("rw-comment-active");
      return;
    }

    const active = items.find((c) => c.id === activeId);
    if (!active || active.selectors.length === 0) {
      highlights.delete("rw-comment-active");
      return;
    }

    const result = selectorsToRange(active.selectors, container);
    if (result) {
      highlights.set("rw-comment-active", new Highlight(result.range));
    } else {
      highlights.delete("rw-comment-active");
    }

    return () => {
      highlights.delete("rw-comment-active");
    };
  });

  function handleMouseUp(event: MouseEvent) {
    const selection = window.getSelection();

    // If no text selected, check if click landed on a comment highlight
    if (!selection || selection.isCollapsed) {
      selectionPopover.clear();

      // Toggle: click an inactive highlight to activate, click the active one to dismiss.
      const hitId = findCommentAtPoint(event);
      if (hitId) comments.activeId = hitId === comments.activeId ? null : hitId;
      return;
    }

    selectionPopover.capture(selection.getRangeAt(0));
  }

  function handleMouseMove(event: MouseEvent) {
    if (!articleRef) return;
    const desired = commentRanges.size > 0 && findCommentAtPoint(event) ? "pointer" : "";
    if (articleRef.style.cursor !== desired) {
      articleRef.style.cursor = desired;
    }
  }

  /** Find which comment (if any) contains the click point. */
  function findCommentAtPoint(event: MouseEvent): string | null {
    if (!articleRef) return null;

    const { clientX, clientY } = event;

    for (const [id, range] of commentRanges) {
      // getClientRects() returns one rect per line for multi-line ranges
      const rects = range.getClientRects();
      for (const rect of rects) {
        if (
          clientX >= rect.left &&
          clientX <= rect.right &&
          clientY >= rect.top &&
          clientY <= rect.bottom
        ) {
          return id;
        }
      }
    }

    return null;
  }

  /** Vertical offset of the anchor point for a comment's highlight, relative to
   *  the article element. The anchor point is the vertical middle of the first
   *  line of highlighted text — multi-line highlights still anchor to the first
   *  line, which is where the reader's eye lands.
   *
   *  When the range's start sits at the boundary between two text nodes (e.g.
   *  end of an inline element right before a sibling code span), browsers can
   *  return a leading zero-width rect at the end of the previous line ahead of
   *  the real highlight — skipping width==0 rects avoids anchoring to that
   *  invisible artifact. */
  function getHighlightTop(commentId: string): number | null {
    const range = commentRanges.get(commentId);
    if (!range || !articleRef) return null;
    const rects = range.getClientRects();
    let firstLineRect: DOMRect | null = null;
    for (const r of rects) {
      if (r.width > 0 && r.height > 0) {
        firstLineRect = r;
        break;
      }
    }
    firstLineRect ??= range.getBoundingClientRect();
    const articleRect = articleRef.getBoundingClientRect();
    return firstLineRect.top + firstLineRect.height / 2 - articleRect.top;
  }

  $effect(() => {
    const pending = comments.pending;
    const container = articleRef;
    if (!pending || !container || pending.selectors.length === 0) {
      comments.pendingTop = null;
      return;
    }
    void articleSize.version;

    const result = selectorsToRange(pending.selectors, container);
    if (result) {
      const rangeRect = result.range.getBoundingClientRect();
      const articleRect = container.getBoundingClientRect();
      comments.pendingTop = rangeRect.top - articleRect.top;
    }
  });

  function handleAddComment() {
    const selection = window.getSelection();
    if (!selection || selection.isCollapsed || !articleRef || !page.data) return;

    const range = selection.getRangeAt(0);
    const selectors = rangeToSelectors(range, articleRef);
    const docId = page.data.meta.path.replace(/^\//, "");

    comments.pending = { documentId: docId, selectors };
    comments.activeId = null;
    selectionPopover.clear();
    window.getSelection()?.removeAllRanges();
  }
</script>

{#if page.loading && showSkeleton}
  <LoadingSkeleton />
{:else if page.loading && page.data}
  <!-- Fast load: show previous content with reduced opacity -->
  <article
    class="
      prose max-w-none opacity-50 transition-opacity duration-150 prose-slate
      dark:prose-invert
    "
  >
    {@html page.data.content}
  </article>
{:else if page.notFound}
  <div class="flex h-64 items-center justify-center">
    <div class="text-center">
      <h1 class="mb-4 text-4xl font-bold tracking-tight text-gray-300 dark:text-neutral-600">
        404
      </h1>
      <p class="text-gray-600 dark:text-neutral-400">Page not found</p>
    </div>
  </div>
{:else if page.error}
  <div class="flex h-64 items-center justify-center">
    <Alert intent="danger" title="Error">{page.error}</Alert>
  </div>
{:else if page.data}
  <!--
    Relative wrapper so the Add-comment popover can anchor `position: absolute`
    to the article — sharing its scroll layer keeps the popover pinned to the
    highlighted text via the compositor, with no JS repositioning on scroll.
  -->
  <div class="relative">
    {#if comments.enabled && selectionPopover.pos}
      <!--
        Free-mode Popover anchored above the current selection.
        `-translate-x-1/2 -translate-y-full` centers the panel above the anchor
        point; the 8px gap is folded into `y` so the primitive's style stays
        generic. `selectionPopover.pos` is article-relative.
      -->
      <Popover
        open
        strategy="absolute"
        x={selectionPopover.pos.x}
        y={selectionPopover.pos.y - 8}
        class="
          -translate-x-1/2 -translate-y-full rounded-lg border border-gray-200 bg-white shadow-lg
          dark:border-neutral-600 dark:bg-neutral-700
        "
      >
        <Button variant="ghost" onclick={handleAddComment}>
          <svg
            class="size-4"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            stroke-width="2"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              d="M7 8h10M7 12h4m1 8l-4-4H5a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v8a2 2 0 01-2 2h-3l-4 4z"
            />
          </svg>
          Add comment
        </Button>
      </Popover>
    {/if}
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <article
      bind:this={articleRef}
      class="prose max-w-none prose-slate dark:prose-invert"
      onmouseup={handleMouseUp}
      onmousemove={handleMouseMove}
    >
      {@html page.data.content}
    </article>
  </div>
  {#if comments.enabled}
    <PageComments />
  {/if}
{/if}
