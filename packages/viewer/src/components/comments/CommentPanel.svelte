<script lang="ts">
  import { tick } from "svelte";
  import type { Comment } from "../../types/comments";
  import { getRwContext } from "$lib/context";
  import CommentThread from "./CommentThread.svelte";
  import CommentForm from "./CommentForm.svelte";
  import { sortByOrder, holdsSlot } from "$lib/comments/navigation";
  import { escapeId } from "$lib/comments/highlight";
  import { restoreFocusToThread, focusReplyTextarea } from "$lib/comments/focus";
  import { SAVE_FAILED_MESSAGE } from "$lib/comments/messages";
  import { createCommentActions } from "$lib/comments/actions";

  const { comments, notify } = getRwContext();
  const actions = createCommentActions(comments, notify);

  interface Props {
    /** When true (margin column), pin each thread/draft to its highlight's
     *  vertical position with padding-top. When false (popover), render with no
     *  padding — the Popover positions the panel itself. */
    pin?: boolean;
  }

  let { pin = true }: Props = $props();

  let threadAnchor = $state<number | null>(null);
  let pendingAnchor = $state<number | null>(null);
  let cardRef = $state<HTMLDivElement | undefined>();
  // The deep-link target we've already moved focus to. Focus is a one-shot
  // landing action: re-entering the same target later via keyboard nav (n/p)
  // must NOT steal focus again. Reset when the deep-link target clears.
  let focusedFor: string | null = null;

  // Stale offset would otherwise flash on the next pending form's first paint.
  $effect(() => {
    if (!comments.pending) pendingAnchor = null;
  });

  // When an inline thread was opened by a deep link, move focus to its card so
  // keyboard / screen-reader users land on it — once, when it first becomes the
  // active target. If the card is hidden (narrow widths where the comments aside
  // is display:none), fall back to focusing the in-article highlight.
  // focus({ preventScroll }) so we don't fight the PageContent scroll that
  // already positioned the passage.
  $effect(() => {
    const id = comments.activeId;
    const linked = comments.linkedId;
    if (linked == null) {
      focusedFor = null;
      return;
    }
    if (id !== linked || focusedFor === linked) return;
    focusedFor = linked;
    void tick().then(() => {
      if (comments.activeId !== id) return;
      if (cardRef && cardRef.offsetParent !== null) {
        cardRef.focus({ preventScroll: true });
        return;
      }
      const ann = document.querySelector<HTMLElement>(
        `article rw-annotation[data-comment-id="${escapeId(id)}"]`,
      );
      if (ann) {
        ann.tabIndex = -1;
        ann.focus({ preventScroll: true });
      }
    });
  });

  // Move focus into the active inline thread's reply box when the user presses
  // `r` (store bumps replyFocusSeq). Baseline captured at creation so the first
  // effect run — which is not an `r` press — does not steal focus. This panel
  // (whether in the wide margin aside or the narrow CommentPopover) only mounts
  // for an active inline thread (or a pending draft, which doesn't bump the
  // counter), so the single reply form under cardRef is the target.
  let lastReplyFocusSeq = comments.replyFocusSeq;
  $effect(() => {
    const seq = comments.replyFocusSeq;
    if (seq === lastReplyFocusSeq) return;
    lastReplyFocusSeq = seq;
    focusReplyTextarea(cardRef?.querySelector("textarea"));
  });

  /** Threads sorted by current document position (live DOM range order from
   *  `comments.order`). Stored TextPositionSelector offsets can't be trusted
   *  for ordering: they're captured at comment-creation time, so comments
   *  created against different edits of the same document live in different
   *  position coordinate systems. Threads not present in `order` (e.g. not
   *  yet anchored) are placed last in creation order.
   *  Resolved threads are hidden unless one is the currently active thread. */
  const orderedThreads = $derived(
    sortByOrder(
      comments.inlineThreads.filter((t) => holdsSlot(t, comments.activeId)),
      comments.order,
    ),
  );

  const activeThread = $derived(orderedThreads.find((t) => t.id === comments.activeId));
  const activeIndex = $derived(orderedThreads.findIndex((t) => t.id === comments.activeId));

  function goToPrev() {
    if (activeIndex > 0) {
      void navigateTo(orderedThreads[activeIndex - 1].id);
    }
  }

  function goToNext() {
    if (activeIndex >= 0 && activeIndex < orderedThreads.length - 1) {
      void navigateTo(orderedThreads[activeIndex + 1].id);
    }
  }

  /** Switch the active thread while keeping the thread visually pinned in place.
   *  The thread anchors to its highlight's vertical position in the article, so
   *  simply changing activeId makes the thread (and its nav buttons) jump — rapid
   *  consecutive prev/next clicks become impossible. Scrolling by the delta
   *  between old and new highlight positions cancels out the thread's movement,
   *  so the button the reader just clicked stays under the cursor. */
  async function navigateTo(nextId: string) {
    const oldTop = comments.activeTop;
    comments.activeId = nextId;
    await tick();
    const newTop = comments.activeTop;
    if (oldTop !== null && newTop !== null) {
      window.scrollBy(0, newTop - oldTop);
    }
  }

  async function handleReply(parentId: string, body: string) {
    // Shared lookup + create + notify-and-rethrow lives in createCommentActions;
    // the focus restoration below is the only part specific to this component,
    // so it stays here. On failure actions.reply rethrows before this runs (the
    // composer keeps the draft); on success we release focus.
    await actions.reply(parentId, body);
    // Reply succeeded: release the textarea onto the (still-open) thread card so
    // n/p navigation resumes. tick() lets the new reply render first.
    await tick();
    restoreFocusToThread(cardRef);
  }

  async function handleNewCommentSubmit(body: string) {
    const pending = comments.pending;
    if (!pending) return;

    let created: Comment;
    try {
      created = await comments.create({
        documentId: pending.documentId,
        body,
        selectors: pending.selectors,
      });
    } catch (e) {
      notify({ intent: "error", message: SAVE_FAILED_MESSAGE });
      throw e;
    }
    comments.clearPending();
    comments.activeId = created.id;
    // Let the new thread's card mount, then move focus to it (it replaces the
    // now-unmounted composer) so n/p navigation resumes.
    await tick();
    restoreFocusToThread(cardRef);
  }

  function handleNewCommentCancel() {
    comments.clearPending();
  }
</script>

{#if comments.pending}
  <div
    style:padding-top={pin
      ? `${Math.max(0, (comments.pendingTop ?? 0) - (pendingAnchor ?? 0))}px`
      : undefined}
    style:visibility={pin && pendingAnchor === null ? "hidden" : "visible"}
  >
    <CommentForm
      onSubmit={handleNewCommentSubmit}
      onCancel={handleNewCommentCancel}
      autofocus
      pinActions
      onAnchor={(o) => (pendingAnchor = o)}
      outerClass="rounded-md border border-gray-200 bg-white p-3 dark:border-neutral-700 dark:bg-neutral-800"
    />
  </div>
{:else if activeThread}
  <div
    bind:this={cardRef}
    tabindex="-1"
    style:padding-top={pin
      ? `${Math.max(0, (comments.activeTop ?? 0) - (threadAnchor ?? 0))}px`
      : undefined}
    style:visibility={pin && threadAnchor === null ? "hidden" : "visible"}
    class="outline-none"
  >
    <!-- Remount per thread so each switch re-seeds the reply draft from the
         store and resets transient submit/failed state. A reused instance would
         carry one thread's draft (and error state) into the next. -->
    {#key activeThread.id}
      <CommentThread
        comment={activeThread}
        replies={comments.replies(activeThread.id)}
        active={true}
        onResolve={actions.resolve}
        onReopen={actions.reopen}
        onReply={handleReply}
        onDelete={actions.remove}
        onRestore={actions.restore}
        onClose={() => {
          comments.activeId = null;
        }}
        nav={{
          index: activeIndex,
          total: orderedThreads.length,
          onPrev: goToPrev,
          onNext: goToNext,
        }}
        onAnchor={(o) => (threadAnchor = o)}
        fuzzy={comments.anchorStrategies.get(activeThread.id) === "fuzzy"}
        initialReplyDraft={comments.replyDrafts[activeThread.id]}
        onReplyDraftChange={comments.setReplyDraft}
      />
    {/key}
  </div>
{/if}
