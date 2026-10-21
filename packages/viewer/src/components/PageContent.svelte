<script lang="ts">
  import { tick, untrack } from "svelte";
  import { getRwContext } from "$lib/context";
  import { initializeTabs } from "$lib/tabs";
  import { refreshImages } from "$lib/refreshImages";
  import { initializeDiagramZoom } from "$lib/diagram/initializeDiagramZoom";
  import { diagramShadowRoots, diagramSource } from "$lib/diagram/source";
  import { rewriteLocalLinks, rewriteSectionRefLinks } from "$lib/sectionRefs";
  import { rangeToSelectors, selectorsToRange } from "$lib/anchoring";
  import {
    captureReadingAnchor,
    restoreReadingAnchor,
    type ReadingAnchor,
  } from "$lib/readingAnchor";
  import { rangeTouchesDiagram } from "$lib/comments/diagram";
  import { annotationSelector, escapeId } from "$lib/comments/highlight";
  import { reconcileHighlights, desiredHighlights } from "$lib/comments/reconcile";
  import {
    buildCommentHash,
    parseCommentHash,
    classifyCommentTarget,
    type CommentTargetKind,
  } from "$lib/comments/deeplink";
  import { isNewlyOrphaned } from "$lib/comments/navigation";
  import { clampPopoverLeft } from "$lib/comments/popoverAnchor";
  import LoadingSkeleton from "$lib/ui/primitives/LoadingSkeleton.svelte";
  import Alert from "$lib/ui/primitives/Alert.svelte";
  import IconButton from "$lib/ui/primitives/IconButton.svelte";
  import { useElementSize } from "$lib/ui/hooks/useElementSize.svelte";
  import { useElementMove } from "$lib/ui/hooks/useElementMove.svelte";
  import { useSelectionPopover } from "$lib/ui/hooks/useSelectionPopover.svelte";
  import { useSeqChange } from "$lib/ui/hooks/useSeqChange.svelte";
  import PageComments from "./comments/PageComments.svelte";
  import CommentPopover from "./comments/CommentPopover.svelte";
  import DiagramZoomModal from "./DiagramZoomModal.svelte";

  const ctx = getRwContext();
  const { page, router, comments, liveReload } = ctx;

  let articleRef: HTMLElement | undefined = $state();
  let loadingArticleRef: HTMLElement | undefined = $state();
  let showSkeleton = $state(false);

  // The zoom popup is keyed on the diagram's identity, not its node, so it survives a
  // live reload replacing the article. `zoomTrackId` re-resolves the figure after a
  // reload; it is null for a hand-authored `<figure class="diagram">`, which opens but
  // does not live-update.
  let zoomId = $state<string | null>(null);
  let zoomFigure = $state<HTMLElement | null>(null);
  let zoomTrackId: string | null = null;
  let zoomAnonSeq = 0;

  function closeZoom() {
    zoomId = null;
    zoomFigure = null;
    zoomTrackId = null;
  }

  // Bumped after a deep-link reveal to re-measure the thread pin: a cold load reflows
  // (web fonts) before any observer reports it.
  let deepLinkSettleSeq = $state(0);

  const docId = $derived(page.data?.meta.documentId ?? null);

  const articleSize = useElementSize(() => articleRef ?? null);

  // Re-aligns the pinned thread when content above the highlight reflows, which the
  // article ResizeObserver can't see.
  const activeHighlightMove = useElementMove(() => {
    // Read `items`: a reconcile can replace the wrapper while activeId is unchanged.
    void comments.items;
    const id = comments.activeId;
    if (!id || !articleRef) return null;
    return articleRef.querySelector<HTMLElement>(annotationSelector(id));
  });

  // Listens for mouseup on the whole document, so a selection released outside the
  // article still opens the popover.
  const selectionPopover = comments.enabled
    ? useSelectionPopover(() => articleRef ?? null, articleSize, rangeTouchesDiagram)
    : null;

  function clearSelection() {
    selectionPopover?.clear();
    window.getSelection()?.removeAllRanges();
  }

  // A content swap detaches the selection's nodes: the cached Range would jump the
  // popover to 0,0, and a later mouseup would compose on empty selectors.
  $effect(() => {
    void page.data;
    clearSelection();
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

  $effect(() => {
    if (page.data && articleRef) {
      return initializeTabs(articleRef);
    }
  });

  // Every bump of `filesVersion` is followed by a page load, which reruns this.
  $effect(() => {
    if (!page.data) return;
    for (const root of [articleRef, loadingArticleRef]) {
      if (root) refreshImages(root, liveReload.filesVersion);
    }
  });

  // Keyed on page.data and articleRef only, so comment mutations don't re-inject buttons.
  $effect(() => {
    if (page.data && articleRef) {
      return initializeDiagramZoom(articleRef, (figure) => {
        const id = figure.dataset.diagramId ?? "";
        zoomTrackId = id.length > 0 ? id : null;
        zoomId = zoomTrackId ?? `anon-${++zoomAnonSeq}`;
        zoomFigure = figure;
      });
    }
  });

  // Keyed on docId, not page.data, so a same-page live reload keeps the popup open.
  // Must stay before the resolve effect, so navigation clears the old id first.
  $effect(() => {
    void docId;
    closeZoom();
  });

  // After a live reload, points the popup at the new `<figure>`. A failed render keeps
  // the last good one and shows a toast; a removed diagram keeps it silently.
  $effect(() => {
    void page.data;
    untrack(() => {
      if (!articleRef || zoomTrackId === null) return;
      const figure = articleRef.querySelector<HTMLElement>(
        `figure.diagram[data-diagram-id="${escapeId(zoomTrackId)}"]`,
      );
      if (figure && !figure.classList.contains("diagram-error") && diagramSource(figure)) {
        zoomFigure = figure;
      } else if (figure) {
        zoomFigure = null;
        ctx.notify({
          intent: "error",
          message: "This diagram can't be shown right now — check its source for errors.",
        });
      }
    });
  });

  // Keeps the reader's place across a live reload. By the pre effect `docId` already
  // names the new page, so a reload is told by the page last shown.
  let shownDocId: string | null = null;
  let reloadAnchor: ReadingAnchor | null = null;
  let stopRestoring = () => {};
  $effect.pre(() => {
    void page.data;
    untrack(() => {
      stopRestoring();
      const isReload = docId === shownDocId;
      reloadAnchor = articleRef && isReload ? captureReadingAnchor(articleRef) : null;
    });
  });
  $effect(() => {
    void page.data;
    untrack(() => {
      shownDocId = docId;
      if (articleRef && reloadAnchor) {
        stopRestoring = restoreReadingAnchor(articleRef, reloadAnchor);
      }
    });
  });

  $effect(() => {
    if (!page.data || !articleRef) return;
    const roots = [articleRef, ...diagramShadowRoots(articleRef)];
    if (page.links.enabled) {
      rewriteSectionRefLinks(roots, page.links, router.basePath, page.data.sectionAncestry);
    } else if (router.embedded) {
      rewriteLocalLinks(roots, router.hostHref);
    }
  });

  // Dedups on linkedId so comments loading or re-anchoring don't re-scroll a target
  // the reader already landed on.
  function landOnComment(id: string): void {
    if (comments.linkedId === id) return; // already landed on this target

    const comment = comments.items.find((c) => c.id === id);
    const kind = classifyCommentTarget(comment, comments.anchors.has(id));
    if (kind === "missing") return; // not loaded yet, or deleted — wait / no-op

    if (kind === "resolved") comments.resolvedExpanded = true;
    // Resolved is left out: a resolved inline thread would wrongly open the sidebar.
    if (kind === "inline" || kind === "page") comments.activeId = id;

    void tick().then(() => {
      if (parseCommentHash(router.hash) !== id) return; // hash moved during the await
      if (comments.linkedId === id) return; // already landed (a racing tick won)
      if (revealCommentTarget(id, kind)) {
        comments.linkedId = id;
        // rAF so the re-measure sees the scrolled layout.
        requestAnimationFrame(() => {
          deepLinkSettleSeq++;
        });
      }
    });
  }

  // Inbound comment deep link (#comment-<id>).
  $effect(() => {
    const id = parseCommentHash(router.hash);

    if (!id) {
      if (comments.linkedId) comments.linkedId = null;
      return;
    }

    // Re-run as comments load / re-anchor.
    void comments.items;
    void comments.anchors;

    landOnComment(id);
  });

  // Outbound: mirror the active thread into the URL. Not through router.hash, which
  // would loop with the inbound effect. replaceState: opening a thread is not a
  // history step. An embedding host's router ignores a hash-only change.
  let mirroredHash: string | null = null;
  $effect(() => {
    const activeId = comments.activeId;

    if (activeId) {
      const hash = buildCommentHash(activeId);
      if (window.location.hash.slice(1) !== hash) {
        history.replaceState(history.state, "", `#${hash}`);
      }
      mirroredHash = hash;
      return;
    }

    // Don't clobber a hash the reader navigated to since.
    if (mirroredHash && window.location.hash.slice(1) === mirroredHash) {
      history.replaceState(history.state, "", window.location.pathname + window.location.search);
    }
    mirroredHash = null;
  });

  $effect(() => {
    if (page.data && comments.enabled && docId !== null) {
      void comments.load(docId);
    } else if (!page.data) {
      comments.clear();
    }
  });

  // Capture `id`: by the time onChange fires, `docId` may have moved to a new page.
  $effect(() => {
    if (docId === null || !comments.enabled) return;
    const id = docId;
    return comments.subscribe(id, () => {
      if (page.data) {
        void comments.load(id, { silent: true });
      }
    });
  });

  // Do NOT replace with reconcile's `anchors`, which dedups: overlapping comments wrap
  // the same id more than once, and the early-out must see the live wrapper set.
  function domWrapperSig(container: HTMLElement): string {
    return [...container.querySelectorAll("rw-annotation")]
      .map((el) => el.getAttribute("data-comment-id") ?? "")
      .join(" ");
  }

  let prevDesiredSig = "";
  let prevDomSig = "";

  // The <rw-annotation> wrappers are reconcile's ledger: it changes only what differs,
  // and a re-render that wiped them gets everything re-anchored.
  $effect(() => {
    const items = comments.items;
    // A resolved comment stays highlighted only while active.
    const activeId = comments.activeId;
    const container = articleRef;
    if (!container) return;

    const sel = window.getSelection();
    const selectionRange = sel && sel.rangeCount > 0 && !sel.isCollapsed ? sel.getRangeAt(0) : null;

    const desired = desiredHighlights(items, activeId);

    // Keeps n/p from re-anchoring on every keypress. The DOM check catches a
    // re-render that wiped the wrappers while `desired` stayed the same.
    const desiredSig = desired.map((d) => d.id).join(" ");
    if (desiredSig === prevDesiredSig && domWrapperSig(container) === prevDomSig) return;

    const result = reconcileHighlights(container, desired, selectionRange);

    comments.anchors = result.anchors;
    comments.orphanIds = result.orphanIds;

    prevDesiredSig = desiredSig;
    prevDomSig = domWrapperSig(container);

    if (result.touchesSelection) {
      // A wrap/unwrap mutated text nodes the selection points into; drop it so a
      // later mouseup can't read a Range over merged/split containers.
      clearSelection();
    }

    // Do NOT add an unwrapping cleanup: it runs before every re-run, turning each
    // mutation into a full rebuild. The wrappers go away with the replaced article.
  });

  // Deactivates a thread whose anchor just vanished (the sidebar hides orphans), but
  // not one the reader stepped onto while already orphaned. Keep apart from the wrap
  // effect, which reads the activeId this writes.
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
    void articleSize.version;
    void activeHighlightMove.version;
    void deepLinkSettleSeq;
    const { pending, activeId } = comments;
    if (!articleRef) {
      comments.surfaceAnchor = null;
    } else if (pending) {
      const range = selectorsToRange(pending.selectors, articleRef)?.range;
      // A draft whose passage a reload removed stays where it was.
      if (range) comments.surfaceAnchor = measureAnchor(range, articleRef);
    } else {
      const range = activeId ? comments.anchors.get(activeId)?.range : undefined;
      comments.surfaceAnchor = range ? measureAnchor(range, articleRef) : null;
    }
  });

  // Also re-runs on items: the wrap effect may have just made fresh wrappers.
  $effect(() => {
    const activeId = comments.activeId;
    void comments.items;
    const container = articleRef;
    if (!container || !activeId) return;

    for (const el of container.querySelectorAll(annotationSelector(activeId))) {
      el.setAttribute("data-active", "true");
    }

    return () => {
      for (const el of container.querySelectorAll("rw-annotation[data-active]")) {
        el.removeAttribute("data-active");
      }
    };
  });

  // Lands where revealCommentTarget does.
  useSeqChange(
    () => comments.navSeq,
    () => {
      const activeId = comments.activeId;
      if (!activeId || !articleRef) return;
      articleRef
        .querySelector(annotationSelector(activeId))
        ?.scrollIntoView({ behavior: "auto", block: "start" });
    },
  );

  // The draft selection is painted with the CSS Highlight API: wrapping it in DOM
  // would fight the live selection.
  $effect(() => {
    const pending = comments.pending;
    const container = articleRef;
    if (!container || typeof CSS === "undefined" || !("highlights" in CSS)) return;

    if (!pending || pending.selectors.length === 0) return;
    const result = selectorsToRange(pending.selectors, container);
    if (!result) return;

    const highlights = CSS.highlights as Map<string, Highlight>;
    highlights.set("rw-comment-active", new Highlight(result.range));
    return () => {
      highlights.delete("rw-comment-active");
    };
  });

  function handleMouseUp(event: MouseEvent) {
    const selection = window.getSelection();

    // The selection hook handles non-collapsed selections.
    if (selection && !selection.isCollapsed) return;

    selectionPopover?.clear();

    // Toggle: click an inactive highlight to activate, click the active one to dismiss.
    const hitId = findCommentAtPoint(event);
    if (hitId) comments.activeId = hitId === comments.activeId ? null : hitId;
  }

  /** Innermost wins: with overlapping comments, the darker inner patch is the more
   *  specific thread. */
  function findCommentAtPoint(event: MouseEvent): string | null {
    const target = event.target;
    if (!(target instanceof Element)) return null;
    return target.closest("rw-annotation")?.getAttribute("data-comment-id") ?? null;
  }

  /** Skips zero-width rects: at a text-node boundary browsers can report one at the
   *  end of the previous line. */
  function measureAnchor(range: Range, article: HTMLElement) {
    const bounds = range.getBoundingClientRect();
    const firstLine =
      [...range.getClientRects()].find((r) => r.width > 0 && r.height > 0) ?? bounds;
    const articleRect = article.getBoundingClientRect();
    const centerX = firstLine.left + firstLine.width / 2 - articleRect.left;
    return {
      top: firstLine.top + firstLine.height / 2 - articleRect.top,
      left: clampPopoverLeft(centerX, article.clientWidth),
      textTop: bounds.top - articleRect.top,
    };
  }

  /** False while the target isn't in the DOM yet; the inbound effect retries.
   *  scrollIntoView works whichever element scrolls. CommentPanel focuses inline
   *  targets. */
  function revealCommentTarget(id: string, kind: CommentTargetKind): boolean {
    if (kind === "inline") {
      const el = articleRef?.querySelector(annotationSelector(id));
      if (!el) return false;
      // With `rw-annotation { scroll-margin-top: 33vh }` the passage lands ~⅓ down,
      // leaving room for the pinned thread; CSS, not scroll math, is embed-safe.
      el.scrollIntoView({ behavior: "auto", block: "start" });
      return true;
    }
    // page / resolved: the timeline thread wrapper carries id="comment-<id>".
    const el = document.getElementById(buildCommentHash(id));
    if (!(el instanceof HTMLElement)) return false;
    el.scrollIntoView({ behavior: "auto", block: "start" });
    el.focus({ preventScroll: true });
    return true;
  }

  function handleAddComment() {
    const selection = window.getSelection();
    if (!selection || selection.isCollapsed || !articleRef || !page.data || docId === null) return;

    const range = selection.getRangeAt(0);
    const selectors = rangeToSelectors(range, articleRef);
    // Empty when the selection holds no prose (whitespace, or inside a diagram).
    if (selectors.length > 0) {
      comments.pending = { documentId: docId, selectors };
      comments.activeId = null;
    }
    clearSelection();
  }
</script>

{#if page.loading && showSkeleton}
  <LoadingSkeleton />
{:else if page.loading && page.data}
  <!-- Fast load: show previous content with reduced opacity -->
  <article
    bind:this={loadingArticleRef}
    class="
      prose max-w-none opacity-50 transition-opacity duration-150 prose-slate
      dark:prose-invert
    "
  >
    <!-- Server-rendered markdown from rw's own renderer, which owns the safety of its output. -->
    <!-- eslint-disable-next-line svelte/no-at-html-tags -->
    {@html page.data.content}
  </article>
{:else if page.notFound}
  <div class="flex h-64 items-center justify-center">
    <div class="text-center">
      <h1 class="mb-4 text-4xl font-bold tracking-tight text-neutral-300 dark:text-neutral-600">
        404
      </h1>
      <p class="text-neutral-600 dark:text-neutral-400">Page not found</p>
    </div>
  </div>
{:else if page.error}
  <div class="flex h-64 items-center justify-center">
    <Alert intent="danger" title="Error">{page.error}</Alert>
  </div>
{:else if page.data}
  <!-- Sharing the article's scroll layer pins the popover to the text without JS. -->
  <div class="relative">
    {#if comments.enabled}
      <CommentPopover />
    {/if}
    {#if selectionPopover?.pos}
      <!-- The translates center the button over `pos` with its bottom edge 8px above. -->
      <div
        class="absolute z-dropdown -translate-x-1/2 -translate-y-full"
        style:left="{selectionPopover.pos.x}px"
        style:top="{selectionPopover.pos.y - 8}px"
      >
        <!-- preventDefault on mousedown keeps the selection alive for the click. -->
        <IconButton
          aria-label="Add comment"
          class="shadow-lg"
          onmousedown={(e) => e.preventDefault()}
          onclick={handleAddComment}
        >
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
        </IconButton>
      </div>
    {/if}
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <article
      bind:this={articleRef}
      class="prose max-w-none prose-slate dark:prose-invert"
      onmouseup={handleMouseUp}
    >
      <!-- Same server-rendered markdown as the loading variant above. -->
      <!-- eslint-disable-next-line svelte/no-at-html-tags -->
      {@html page.data.content}
    </article>
  </div>
  {#if comments.enabled}
    <PageComments />
  {/if}
{/if}

<!-- Outside the page-state branches: a live reload swaps branches, and a remount
     would reset the popup's zoom. -->
<DiagramZoomModal diagramId={zoomId} figure={zoomFigure} onClose={closeZoom} />
