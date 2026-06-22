<script lang="ts">
  import { tick, untrack } from "svelte";
  import { getRwContext } from "$lib/context";
  import { initializeTabs } from "$lib/tabs";
  import { rewriteSectionRefLinks } from "$lib/sectionRefs";
  import { rangeToSelectors, selectorsToRange } from "$lib/anchoring";
  import { escapeId } from "$lib/comments/highlight";
  import { reconcileHighlights } from "$lib/comments/reconcile";
  import {
    buildCommentHash,
    parseCommentHash,
    isCommentHash,
    classifyCommentTarget,
    type CommentTargetKind,
  } from "$lib/comments/deeplink";
  import { isNewlyOrphaned } from "$lib/comments/navigation";
  import { documentIdFor } from "$lib/comments/documentId";
  import LoadingSkeleton from "$lib/ui/primitives/LoadingSkeleton.svelte";
  import Alert from "$lib/ui/primitives/Alert.svelte";
  import Button from "$lib/ui/primitives/Button.svelte";
  import Popover from "$lib/ui/primitives/Popover.svelte";
  import { useElementSize } from "$lib/ui/hooks/useElementSize.svelte";
  import { useSelectionPopover } from "$lib/ui/hooks/useSelectionPopover.svelte";
  import { useScrollIntoViewOnNav } from "$lib/ui/hooks/useScrollIntoViewOnNav.svelte";
  import PageComments from "./comments/PageComments.svelte";

  const ctx = getRwContext();
  const { page, router, comments, liveReload } = ctx;

  let articleRef: HTMLElement | undefined = $state();
  let showSkeleton = $state(false);

  const docId = $derived(page.data ? documentIdFor(page.data.meta) : null);

  const articleSize = useElementSize(() => articleRef ?? null);

  // Text-selection state for the Add-comment popover. The hook owns the
  // captured Range, the article-relative anchor point, and dismiss-on-collapse.
  const selectionPopover = useSelectionPopover(() => articleRef ?? null, articleSize);

  // Drop any in-flight selection when the article content changes (live reload,
  // navigation). The cached Range's start/end nodes get detached, so its rect
  // collapses to zero and would briefly jump the popover to the top-left
  // corner before anything else dismissed it. Also drop the native selection:
  // the reconcile effect only clears it when a wrap/unwrap overlaps it, so a
  // content swap (which detaches the selection's nodes) must clear it here, or a
  // later mouseup could read a Range whose containers are gone and open the
  // composer on empty selectors.
  $effect(() => {
    void page.data;
    selectionPopover.clear();
    window.getSelection()?.removeAllRanges();
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

  // Scroll heading anchors when content loads or the hash changes. A hash that
  // matches a known loaded comment is handled by the inbound deep-link effect
  // below, so bail before the getElementById lookup for those. A heading slug
  // that merely starts with `comment-` (e.g. `## Comment guidelines` →
  // `#comment-guidelines`) is not a known comment id, so it still scrolls here.
  //
  // The comment-id membership check reads `comments.items` with `untrack` so this
  // effect depends only on the hash / page / article ref — not on the comment
  // list. Otherwise loading, creating, or resolving a comment would re-run it and
  // re-scroll a heading the reader has already scrolled past.
  $effect(() => {
    const currentHash = router.hash;
    if (
      isCommentHash(
        currentHash,
        untrack(() => comments.items.map((c) => c.id)),
      )
    )
      return;
    if (page.data && articleRef && currentHash) {
      const target = document.getElementById(currentHash);
      if (target) {
        requestAnimationFrame(() => {
          target.scrollIntoView({ behavior: "auto" });
        });
      }
    }
  });

  // Reveal a deep-link target: classify it, set the comment state that surfaces
  // it (resolved disclosure / active thread), then scroll+focus after the DOM
  // settles. Marking it active also lets keyboard nav (n/p) continue from here,
  // with the tint following the active comment as the reader steps through.
  // Dedups on comments.linkedId so a reactive re-run (comments loading, the
  // inbound effect firing alongside a host that also bridges popstate into
  // router.hash) does not re-scroll a target the reader already landed on. An
  // explicit re-navigation that *should* re-reveal the current target (the
  // popstate handler below) clears linkedId first to opt out of this dedup.
  // `stillCurrent` is re-checked after the await so a hash that moved mid-tick
  // does not land on a stale target.
  function landOnComment(id: string, stillCurrent: () => boolean): void {
    if (comments.linkedId === id) return; // already landed on this target

    const comment = comments.items.find((c) => c.id === id);
    const kind = classifyCommentTarget(comment, comments.order.includes(id));
    if (kind === "missing") return; // not loaded yet, or deleted — wait / no-op

    if (kind === "resolved") comments.resolvedExpanded = true;
    // Resolved is left out: a resolved inline thread would wrongly open the sidebar.
    if (kind === "inline" || kind === "page") comments.activeId = id;

    void tick().then(() => {
      if (!stillCurrent()) return; // hash moved during the await
      if (comments.linkedId === id) return; // already landed (a racing tick won)
      if (revealCommentTarget(id, kind)) {
        comments.linkedId = id;
      }
    });
  }

  // Inbound comment deep-link (#comment-<id>). Driven by router.hash — the same
  // dependency as heading deep-linking, so it works in embedded mode too, as long
  // as the host passes the full path+hash to navigateTo (the preview shell does).
  // Re-runs as comments load / re-anchor; clears linkedId when the hash leaves a
  // comment. The reveal itself is delegated to landOnComment.
  $effect(() => {
    const id = parseCommentHash(router.hash);

    if (!id) {
      if (comments.linkedId) comments.linkedId = null;
      return;
    }

    // Re-run as comments load / re-anchor.
    void comments.items;
    void comments.order;

    landOnComment(id, () => parseCommentHash(router.hash) === id);
  });

  // Same-session inbound for embedded mode: the router ignores popstate here (the
  // host owns path routing), so Back/Forward and manual hash edits would not
  // re-focus a comment. Mirror the heading popstate handler in Layout and reuse
  // landOnComment, reading window.location.hash directly. A host that bridges
  // popstate into router.hash makes the inbound effect fire too, but landOnComment
  // dedups on linkedId so the two paths don't double-scroll.
  $effect(() => {
    if (!router.embedded) return;
    function onPopState() {
      const id = parseCommentHash(window.location.hash);
      if (!id) return;
      // A Back/Forward or manual hash edit is an explicit re-navigation: re-reveal
      // the target even when it is already the linked comment (the heading popstate
      // handler likewise always re-scrolls). landOnComment dedups on linkedId, so
      // clear it for the already-linked target to allow the re-reveal.
      if (comments.linkedId === id) comments.linkedId = null;
      landOnComment(id, () => parseCommentHash(window.location.hash) === id);
    }
    window.addEventListener("popstate", onPopState);
    return () => window.removeEventListener("popstate", onPopState);
  });

  // Outbound: mirror the open inline/active thread into the address bar (both
  // standalone and embedded). Uses replaceState — opening a thread is not history
  // navigation, so Back/Forward does not step through opened threads. Writes
  // window.location directly (not router.hash): replaceState does not fire
  // popstate, and the router only updates router.hash from popstate, so this
  // write stays invisible to the inbound effect. Writing router.hash (or
  // comments.linkedId) here would instead form a dual-writer loop with that
  // effect. In embedded mode this writes the host page's URL hash, which the host
  // router ignores — a hash-only change does not touch the path it routes on.
  let mirroredHash: string | null = null;
  $effect(() => {
    const activeId = comments.activeId;

    if (activeId) {
      const hash = buildCommentHash(activeId);
      if (window.location.hash.slice(1) !== hash) {
        history.replaceState(null, "", `#${hash}`);
      }
      mirroredHash = hash;
      return;
    }

    // Closed: clear the hash only if the URL still shows the thread we mirrored
    // (don't clobber a heading hash the user navigated to in the meantime).
    if (mirroredHash && window.location.hash.slice(1) === mirroredHash) {
      history.replaceState(null, "", window.location.pathname + window.location.search);
    }
    mirroredHash = null;
  });

  // Load comments when page data changes or comments become enabled.
  // On initial load, config may not have arrived yet — reading
  // comments.enabled ensures the effect re-runs when it flips to true.
  $effect(() => {
    if (page.data && comments.enabled && docId !== null) {
      comments.load(docId);
    } else if (!page.data) {
      comments.clear();
    }
  });

  // Live refresh. Prefer the comment client's own transport when it provides
  // one (injected hosts); otherwise fall back to the live-reload WebSocket.
  //
  // Two separate effects, not one branch: the WebSocket subscriber must register
  // exactly once at mount and stay registered across navigation. `comments.canSubscribe`
  // reads `apiClient.subscribe` — a plain field fixed for the instance's life, so
  // Svelte never tracks it and this effect runs once; the callback closes over
  // `docId` and reads its latest value each time it fires. Do NOT make
  // `canSubscribe` reactive here: that would tear down and re-register the
  // WebSocket listener on every page change.
  $effect(() => {
    if (comments.canSubscribe) return;
    return liveReload.onCommentsReload(() => {
      if (page.data && comments.enabled && docId !== null) {
        comments.load(docId, { silent: true });
      }
    });
  });

  // The client's own transport is per-document, so re-subscribe whenever `docId`
  // changes (read reactively here). Capture it into `id` first: by the time the
  // onChange callback fires, the outer `docId` may have advanced to a new page.
  $effect(() => {
    if (!comments.canSubscribe || docId === null) return;
    const id = docId;
    return comments.subscribe(id, () => {
      if (page.data && comments.enabled) {
        comments.load(id, { silent: true });
      }
    });
  });

  // Map from comment ID to its anchored Range (for click detection)
  let commentRanges = $state.raw<Map<string, Range>>(new Map());

  // Reconcile comment highlights to the current open/anchored set. The
  // <rw-annotation> DOM is the wrapped-set ledger, so this mutates only what
  // changed (resolve unwraps one; create/reopen wraps one) instead of tearing
  // the whole article down and rebuilding it on every comment mutation. A
  // navigation / live-reload re-render leaves zero wrappers, so the same pass
  // re-anchors everything. The user's text selection is preserved unless a
  // wrap/unwrap actually overlaps it. Overlapping comments nest so the
  // box-model alpha compositing produces a darker highlight where they overlap;
  // the active comment uses CSS.highlights.rw-comment-active (next effect)
  // because a single-range overlay doesn't need DOM mutation.
  $effect(() => {
    const items = comments.items;
    const container = articleRef;
    if (!container) return;

    const sel = window.getSelection();
    const selectionRange = sel && sel.rangeCount > 0 && !sel.isCollapsed ? sel.getRangeAt(0) : null;

    const desired = items
      .filter((c) => c.status !== "resolved" && c.selectors.length > 0)
      .map((c) => ({ id: c.id, selectors: c.selectors, parentId: c.parentId }));

    const result = reconcileHighlights(container, desired, selectionRange);

    commentRanges = result.ranges;
    comments.anchorStrategies = result.strategies;
    comments.orphanIds = result.orphanIds;
    comments.order = result.order;

    if (result.touchesSelection) {
      // A wrap/unwrap mutated text nodes the selection points into; drop it so a
      // later mouseup can't read a Range over merged/split containers.
      selectionPopover.clear();
      window.getSelection()?.removeAllRanges();
    }

    // No cleanup that unwraps: a Svelte effect cleanup runs before EVERY re-run,
    // so unwrapping here would strip all highlights before each reconcile —
    // turning every comment mutation back into a full teardown + rebuild (and
    // collapsing overlapping comments, since they'd all be re-wrapped in one
    // pass). reconcile reads the live DOM as its ledger and removes only what a
    // change drops; on navigation / unmount the wrappers vanish with the
    // replaced/destroyed article subtree, so there is nothing to clean up.
  });

  // De-activate a thread that *spontaneously* lost its anchor (e.g. live-reload
  // rewrote the page and the active passage is gone): the sidebar filters out
  // orphans, so the open panel would otherwise resolve to nothing. Only a
  // genuine non-orphan → orphan transition clears activeId — navigating *onto*
  // an already-orphaned comment (it lives in the page-comments timeline and is
  // a valid n/p target) keeps it active so it highlights and stepping continues.
  // Must stay separate from the wrap effect above: that effect reads activeId,
  // so folding this in would re-wrap the article DOM on every n/p keypress.
  // `prevOrphans` is a deliberately non-reactive local: this effect re-runs on
  // orphanIds / activeId changes, not on its own bookkeeping write.
  let prevOrphans = new Set<string>();
  $effect(() => {
    const orphans = comments.orphanIds;
    const activeId = comments.activeId;
    if (isNewlyOrphaned(activeId, orphans, prevOrphans)) {
      comments.activeId = null;
    }
    prevOrphans = orphans;
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

  // Toggle data-active="true" on the wrappers belonging to the active comment.
  // Cheap attribute toggle, no DOM mutation beyond setAttribute/removeAttribute.
  // Re-runs when activeId flips (the common case) and also when items change,
  // because the wrap effect above may have just produced fresh wrapper elements
  // that still need the attribute applied. Resolved comments aren't wrapped
  // (the wrap effect skips them), so activating a resolved-from-the-sidebar
  // comment has no in-article visual — the sidebar is the indicator.
  $effect(() => {
    const activeId = comments.activeId;
    void comments.items;
    const container = articleRef;
    if (!container) return;

    for (const el of container.querySelectorAll("rw-annotation[data-active]")) {
      el.removeAttribute("data-active");
    }
    if (!activeId) return;

    const escId = escapeId(activeId);
    for (const el of container.querySelectorAll(`rw-annotation[data-comment-id="${escId}"]`)) {
      el.setAttribute("data-active", "true");
    }

    return () => {
      for (const el of container.querySelectorAll("rw-annotation[data-active]")) {
        el.removeAttribute("data-active");
      }
    };
  });

  // Scroll the active inline highlight into view on keyboard navigation.
  useScrollIntoViewOnNav(
    () => comments.navSeq,
    () => {
      const activeId = comments.activeId;
      if (!activeId || !articleRef) return null;
      return articleRef.querySelector(`rw-annotation[data-comment-id="${escapeId(activeId)}"]`);
    },
  );

  // Pending-selection overlay — paint the user's in-progress text selection
  // (while drafting a new comment) via the CSS Custom Highlight API. We can't
  // wrap a draft selection in a DOM element: the user is mid-drag and DOM
  // mutation would fight `window.getSelection()`. Stored comments take the
  // data-active path above instead.
  $effect(() => {
    const pending = comments.pending;
    const container = articleRef;
    if (!container || typeof CSS === "undefined" || !("highlights" in CSS)) return;

    const highlights = CSS.highlights as Map<string, Highlight>;

    if (!pending || pending.selectors.length === 0) {
      highlights.delete("rw-comment-active");
      return;
    }

    const result = selectorsToRange(pending.selectors, container);
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
    const desired = findCommentAtPoint(event) ? "pointer" : "";
    if (articleRef.style.cursor !== desired) {
      articleRef.style.cursor = desired;
    }
  }

  /** Find which comment (if any) the click landed on.
   *
   *  Walks up from `event.target` and returns the *innermost* `<rw-annotation>`
   *  wrapper's `data-comment-id`. Innermost is intentional: when two comments
   *  overlap, the inner one (visually a darker yellow because of nested
   *  alpha-compositing) becomes the topmost DOM node at that point, so
   *  clicking the darker patch picks the more-specific thread.
   *
   *  Resolved-active comments don't get a wrapper (the comment-wrap effect
   *  skips them), so clicking the active overlay over a resolved comment
   *  returns null and won't deactivate via article click — the sidebar's
   *  close button is the dismiss path for resolved-active.
   */
  function findCommentAtPoint(event: MouseEvent): string | null {
    const target = event.target;
    if (!(target instanceof Element)) return null;
    return target.closest("rw-annotation")?.getAttribute("data-comment-id") ?? null;
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
   *  invisible artifact.
   *
   *  When `commentId` doesn't have a wrapper-backed Range (the active comment
   *  just got resolved → wrap effect dropped its wrapper → commentRanges no
   *  longer maps it), fall back to resolving the comment's selectors on
   *  demand. The sidebar thread should stay anchored to the article passage
   *  even after the in-article highlight disappears. */
  function getHighlightTop(commentId: string): number | null {
    if (!articleRef) return null;
    let range = commentRanges.get(commentId);
    if (!range) {
      const comment = comments.items.find((c) => c.id === commentId);
      if (!comment || comment.selectors.length === 0) return null;
      const result = selectorsToRange(comment.selectors, articleRef);
      if (!result) return null;
      range = result.range;
    }
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

  /** Scroll the deep-link target into view and move focus. Returns whether a
   *  target element was found (false means "not in the DOM yet" — the inbound
   *  effect will retry on the next comment-state change). Uses only
   *  scrollIntoView so it works whether window or a host element is the scroller.
   *  Inline focus is delegated to CommentSidebar (it owns its card). */
  function revealCommentTarget(id: string, kind: CommentTargetKind): boolean {
    if (kind === "inline") {
      const el = articleRef?.querySelector(`rw-annotation[data-comment-id="${escapeId(id)}"]`);
      if (!el) return false;
      // Center (not start) — matches keyboard nav, and leaves room above the
      // passage for the sidebar thread, which pins its header to the highlight.
      el.scrollIntoView({ behavior: "auto", block: "center" });
      return true;
    }
    // page / resolved: the timeline thread wrapper carries id="comment-<id>".
    const el = document.getElementById(buildCommentHash(id));
    if (!(el instanceof HTMLElement)) return false;
    el.scrollIntoView({ behavior: "auto", block: "start" });
    el.focus({ preventScroll: true });
    return true;
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
    if (!selection || selection.isCollapsed || !articleRef || !page.data || docId === null) return;

    const range = selection.getRangeAt(0);
    const selectors = rangeToSelectors(range, articleRef);

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
