<script lang="ts">
  import { getRwContext } from "$lib/context";
  import Popover from "$lib/ui/primitives/Popover.svelte";
  import CommentPanel from "./CommentPanel.svelte";
  import { COMMENT_POPOVER_WIDTH_PX } from "$lib/comments/popoverAnchor";

  const { ui, comments } = getRwContext();

  // Must be mounted inside PageContent's `position: relative` article wrapper:
  // the Popover positions the card with `strategy="absolute"` against the nearest
  // positioned ancestor, and the anchor coords are article-relative. Mounted
  // elsewhere it would anchor against the wrong box.

  // Gap (px) between the highlighted line and the top of the popover card.
  const GAP_PX = 12;

  // Show the popover only at narrow widths, when an inline thread is active or a
  // new-comment draft is pending. At >=952px the in-flow margin aside renders the
  // panel instead (Layout gates the aside on !ui.narrow), so the two never both
  // mount the same CommentPanel.
  const show = $derived(ui.narrow && comments.inlineSurfaceActive);

  // Article-relative anchor maintained by PageContent: vertical position of the
  // highlighted line, and a horizontal left clamped so the fixed-width popover
  // centers on the highlight without running off the article edge. Pending takes
  // precedence: while drafting there is no active thread yet.
  const anchorY = $derived((comments.pending ? comments.pendingTop : comments.activeTop) ?? 0);
  const anchorX = $derived((comments.pending ? comments.pendingLeft : comments.activeLeft) ?? 0);

  function close() {
    comments.activeId = null;
    comments.clearPending();
  }

  // Dismissal handled here, not via the shared `dismissible` helper / Popover's
  // `dismissible`, because this popover needs two behaviors the generic helper
  // lacks: (1) exempt clicks on a highlight — those are owned by PageContent's
  // handler, which toggles the active highlight closed or switches to a different
  // one (without the exemption, clicking a second highlight would dismiss instead
  // of switch); and (2) leave Escape to CommentForm when it preventDefaults (so
  // Escape in the reply/draft box blurs the field instead of closing the thread).
  function dismiss(node: HTMLElement) {
    // Where focus was before the popover opened, so an Escape-close can return
    // it there instead of dropping keyboard users on <body>.
    const restoreEl = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    function onClick(event: MouseEvent) {
      const target = event.target as Element | null;
      if (node.contains(target)) return;
      if (target?.closest("rw-annotation")) return;
      close();
    }
    function onKeydown(event: KeyboardEvent) {
      // CommentForm preventDefaults Escape to blur its field / cancel a draft, and
      // skips that while an IME composition is active (so Escape cancels the
      // composition). Honor both: only a bare Escape — focus on the card, no IME —
      // closes the thread, then restore focus to where it was before opening.
      if (event.key === "Escape" && !event.defaultPrevented && !event.isComposing) {
        close();
        // Only restore to a still-connected element — after several thread
        // switches the captured node may have been replaced; focusing a detached
        // node is a no-op that would silently drop focus on <body>.
        if (restoreEl?.isConnected) restoreEl.focus();
      }
    }
    document.addEventListener("click", onClick, true);
    window.addEventListener("keydown", onKeydown);
    return () => {
      document.removeEventListener("click", onClick, true);
      window.removeEventListener("keydown", onKeydown);
    };
  }
</script>

{#if show}
  <Popover open strategy="absolute" x={anchorX} y={anchorY + GAP_PX}>
    <!-- No card chrome of its own: CommentPanel's thread/draft already draws the
         bordered, padded card, so the wrapper only adds the floating shadow, the
         fixed width, and the scroll cap. mb-6 keeps a gap below the popover when
         it is the bottom-most thing on the page (a tall thread near the end): the
         absolute element's bottom margin extends the page's scrollable area. -->
    <!-- A labelled `group`, not `dialog`: this is a non-modal comments surface that
         does not trap focus or move focus into itself on open (tapping a highlight /
         `n`), so `dialog` (which implies that focus management) would over-promise.
         `group` gives an accessible name and a role-based test handle without the
         contract — and, unlike `region`, doesn't collide with the page-comments
         `<section>` landmark, which is already a region named "Comments". -->
    <div
      {@attach dismiss}
      role="group"
      aria-label="Comments"
      style:width="{COMMENT_POPOVER_WIDTH_PX}px"
      class="mb-6 max-h-[calc(100dvh-5rem)] overflow-y-auto rounded-md shadow-lg"
    >
      <CommentPanel pin={false} />
    </div>
  </Popover>
{/if}
