<script lang="ts">
  import { tick } from "svelte";
  import { getRwContext } from "$lib/context";
  import CommentThread from "./CommentThread.svelte";
  import CommentForm from "./CommentForm.svelte";
  import { annotationSelector } from "$lib/comments/highlight";
  import { restoreFocusToThread, focusReplyTextarea } from "$lib/comments/focus";
  import { createCommentActions } from "$lib/comments/actions";
  import { useSeqChange } from "$lib/ui/hooks/useSeqChange.svelte";

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
      const ann = document.querySelector<HTMLElement>(`article ${annotationSelector(id)}`);
      if (ann) {
        ann.tabIndex = -1;
        ann.focus({ preventScroll: true });
      }
    });
  });

  // Move focus into the active inline thread's reply box on `r`. This panel only
  // mounts for an active inline thread (or a pending draft, which doesn't bump
  // the counter), so the single reply form under cardRef is the target.
  useSeqChange(
    () => comments.replyFocusSeq,
    () => focusReplyTextarea(cardRef?.querySelector("textarea")),
    true,
  );

  const orderedThreads = $derived(comments.orderedInlineThreads);

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

  async function handleNewCommentSubmit(body: string) {
    const pending = comments.pending;
    if (!pending) return;

    const created = await actions.create({
      documentId: pending.documentId,
      body,
      selectors: pending.selectors,
    });
    comments.clearPending();
    comments.activeId = created.id;
    // Let the new thread's card mount, then move focus to it (it replaces the
    // now-unmounted composer) so n/p navigation resumes.
    await tick();
    restoreFocusToThread(cardRef);
  }

  // Hidden until the card reports its anchor so it never paints at a stale offset.
  function pinStyle(top: number | null, anchor: number | null): string {
    if (!pin) return "visibility: visible";
    const paddingTop = Math.max(0, (top ?? 0) - (anchor ?? 0));
    return `padding-top: ${paddingTop}px; visibility: ${anchor === null ? "hidden" : "visible"}`;
  }
</script>

{#if comments.pending}
  <div style={pinStyle(comments.pendingTop, pendingAnchor)}>
    <CommentForm
      onSubmit={handleNewCommentSubmit}
      onCancel={comments.clearPending}
      autofocus
      pinActions
      onAnchor={(o) => (pendingAnchor = o)}
      outerClass="rounded-md border border-border-default bg-bg-raised p-3"
    />
  </div>
{:else if activeThread}
  <div
    bind:this={cardRef}
    tabindex="-1"
    style={pinStyle(comments.activeTop, threadAnchor)}
    class="outline-none"
  >
    <!-- Remount per thread so each switch re-seeds the reply draft from the
         store and resets transient submit/failed state. A reused instance would
         carry one thread's draft (and error state) into the next. -->
    {#key activeThread.id}
      <CommentThread
        comment={activeThread}
        replies={comments.replies(activeThread.id)}
        onResolve={actions.resolve}
        onReopen={actions.reopen}
        onReply={actions.reply}
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
