<script lang="ts">
  import { getRwContext } from "../lib/context";
  import { initializeTabs } from "../lib/tabs";
  import { rewriteSectionRefLinks } from "../lib/sectionRefs";
  import { rangeToSelectors, selectorsToRange, type AnchorStrategy } from "../lib/anchoring";
  import { LOADING_SHOW_DELAY } from "../lib/constants";
  import LoadingSkeleton from "./LoadingSkeleton.svelte";
  import SelectionPopover from "./comments/SelectionPopover.svelte";
  import PageComments from "./comments/PageComments.svelte";

  const ctx = getRwContext();
  const { page, router, comments } = ctx;

  let articleRef: HTMLElement | undefined = $state();
  let showSkeleton = $state(false);

  // Comment selection state
  let selectionRect: { x: number; y: number } | null = $state(null);

  // Dismiss the popover when the selection collapses (e.g. user clicks on the
  // selected text). Blink runs the click-on-selection collapse as a default
  // action of `click`, so reading window.getSelection() inside `mouseup`
  // still returns the active range — handleMouseUp would re-pin the popover
  // at the same coords and only the highlight would disappear.
  $effect(() => {
    if (!selectionRect) return;
    const handler = () => {
      const sel = window.getSelection();
      if (!sel || sel.isCollapsed) selectionRect = null;
    };
    document.addEventListener("selectionchange", handler);
    return () => document.removeEventListener("selectionchange", handler);
  });

  // Show skeleton only if loading takes longer than SHOW_DELAY
  $effect(() => {
    if (page.loading) {
      const timeout = setTimeout(() => {
        showSkeleton = true;
      }, LOADING_SHOW_DELAY);
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

  // Apply comment highlights via CSS Custom Highlight API
  $effect(() => {
    const items = comments.items;
    const container = articleRef;
    if (!container || typeof CSS === "undefined" || !("highlights" in CSS)) return;

    const highlights = CSS.highlights as Map<string, Highlight>;
    const exactRanges: Range[] = [];
    const fuzzyRanges: Range[] = [];
    const rangeMap = new Map<string, Range>();
    const strategyMap = new Map<string, AnchorStrategy>();
    const orphanIds = new Set<string>();
    const anchored: { id: string; range: Range }[] = [];

    for (const comment of items) {
      if (comment.selectors.length === 0) continue;
      if (comment.status === "resolved" && comment.id !== comments.activeId) continue;
      const result = selectorsToRange(comment.selectors, container);
      if (result) {
        if (result.strategy === "fuzzy") {
          fuzzyRanges.push(result.range);
        } else {
          exactRanges.push(result.range);
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

    if (exactRanges.length > 0) {
      highlights.set("rw-comments", new Highlight(...exactRanges));
    } else {
      highlights.delete("rw-comments");
    }
    if (fuzzyRanges.length > 0) {
      highlights.set("rw-comments-fuzzy", new Highlight(...fuzzyRanges));
    } else {
      highlights.delete("rw-comments-fuzzy");
    }

    return () => {
      highlights.delete("rw-comments");
      highlights.delete("rw-comments-fuzzy");
    };
  });

  // Keep activeTop in sync with activeId — recomputes whenever either changes.
  // Article reflow (window resize, font load, sidebar open/close) shifts the
  // highlight vertically inside the article, so a ResizeObserver re-measures
  // the offset whenever the article changes size.
  $effect(() => {
    const activeId = comments.activeId;
    const container = articleRef;
    if (!activeId || !container) {
      comments.activeTop = null;
      return;
    }
    comments.activeTop = getHighlightTop(activeId);

    const observer = new ResizeObserver(() => {
      comments.activeTop = getHighlightTop(activeId);
    });
    observer.observe(container);
    return () => observer.disconnect();
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
      selectionRect = null;

      // Toggle: click an inactive highlight to activate, click the active one to dismiss.
      const hitId = findCommentAtPoint(event);
      if (hitId) comments.activeId = hitId === comments.activeId ? null : hitId;
      return;
    }

    const range = selection.getRangeAt(0);
    if (!articleRef || !articleRef.contains(range.commonAncestorContainer)) {
      selectionRect = null;
      return;
    }

    const rect = range.getBoundingClientRect();
    selectionRect = { x: rect.left + rect.width / 2, y: rect.top };
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

  // Keep pendingTop in sync — recalculates when the sidebar opens and whenever
  // the article resizes, since content reflow moves the anchored range.
  $effect(() => {
    const pending = comments.pending;
    const container = articleRef;
    if (!pending || !container || pending.selectors.length === 0) {
      comments.pendingTop = null;
      return;
    }

    const measure = () => {
      const result = selectorsToRange(pending.selectors, container);
      if (result) {
        const rangeRect = result.range.getBoundingClientRect();
        const articleRect = container.getBoundingClientRect();
        comments.pendingTop = rangeRect.top - articleRect.top;
      }
    };

    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(container);
    return () => observer.disconnect();
  });

  function handleAddComment() {
    const selection = window.getSelection();
    if (!selection || selection.isCollapsed || !articleRef || !page.data) return;

    const range = selection.getRangeAt(0);
    const selectors = rangeToSelectors(range, articleRef);
    const docId = page.data.meta.path.replace(/^\//, "");

    comments.pending = { documentId: docId, selectors };
    comments.activeId = null;
    selectionRect = null;
    window.getSelection()?.removeAllRanges();
  }
</script>

{#if comments.enabled && selectionRect}
  <SelectionPopover x={selectionRect.x} y={selectionRect.y} onAdd={handleAddComment} />
{/if}

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
    <p class="text-red-600 dark:text-red-400">Error: {page.error}</p>
  </div>
{:else if page.data}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <article
    bind:this={articleRef}
    class="prose max-w-none prose-slate dark:prose-invert"
    onmouseup={handleMouseUp}
    onmousemove={handleMouseMove}
  >
    {@html page.data.content}
  </article>
  {#if comments.enabled}
    <PageComments />
  {/if}
{/if}
