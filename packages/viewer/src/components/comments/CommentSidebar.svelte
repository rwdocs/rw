<script lang="ts">
  import { tick } from "svelte";
  import type { Comment } from "../../types/comments";
  import { getRwContext } from "../../lib/context";
  import CommentThread from "./CommentThread.svelte";
  import CommentForm from "./CommentForm.svelte";
  import Alert from "../../lib/ui/primitives/Alert.svelte";

  const { comments } = getRwContext();

  let threadAnchor = $state<number | null>(null);
  let pendingAnchor = $state<number | null>(null);

  // Stale offset would otherwise flash on the next pending form's first paint.
  $effect(() => {
    if (!comments.pending) pendingAnchor = null;
  });

  /** Threads sorted by current document position (live DOM range order from
   *  `comments.order`). Stored TextPositionSelector offsets can't be trusted
   *  for ordering: they're captured at comment-creation time, so comments
   *  created against different edits of the same document live in different
   *  position coordinate systems. Threads not present in `order` (e.g. not
   *  yet anchored) are placed last in creation order.
   *  Resolved threads are hidden unless one is the currently active thread. */
  const orderedThreads = $derived.by(() => {
    const filtered = comments.inlineThreads.filter(
      (t) => t.status !== "resolved" || t.id === comments.activeId,
    );
    const rank = new Map(comments.order.map((id, i) => [id, i]));
    const indexOf = (t: Comment) => rank.get(t.id) ?? Infinity;
    return filtered.toSorted((a, b) => indexOf(a) - indexOf(b));
  });

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

  async function handleResolve(id: string) {
    try {
      await comments.resolve(id);
    } catch (e) {
      comments.error = e instanceof Error ? e.message : "Failed to resolve comment";
    }
  }

  async function handleReopen(id: string) {
    try {
      await comments.reopen(id);
    } catch (e) {
      comments.error = e instanceof Error ? e.message : "Failed to reopen comment";
    }
  }

  async function handleReply(parentId: string, body: string) {
    const thread = comments.threads.find((t) => t.id === parentId);
    if (!thread) return;
    try {
      await comments.create({
        documentId: thread.documentId,
        parentId,
        body,
        selectors: [],
      });
    } catch (e) {
      comments.error = e instanceof Error ? e.message : "Failed to add reply";
    }
  }

  async function handleNewCommentSubmit(body: string) {
    const pending = comments.pending;
    if (!pending) return;

    try {
      const created = await comments.create({
        documentId: pending.documentId,
        body,
        selectors: pending.selectors,
      });
      comments.clearPending();
      comments.activeId = created.id;
    } catch (e) {
      comments.error = e instanceof Error ? e.message : "Failed to create comment";
    }
  }

  function handleNewCommentCancel() {
    comments.clearPending();
  }
</script>

{#if comments.error}
  <Alert intent="danger" dismissible onDismiss={() => (comments.error = null)} class="mb-2">
    {comments.error}
  </Alert>
{/if}

{#if comments.pending}
  <div
    style:padding-top="{Math.max(0, (comments.pendingTop ?? 0) - (pendingAnchor ?? 0))}px"
    style:visibility={pendingAnchor === null ? "hidden" : "visible"}
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
    style:padding-top="{Math.max(0, (comments.activeTop ?? 0) - (threadAnchor ?? 0))}px"
    style:visibility={threadAnchor === null ? "hidden" : "visible"}
  >
    <CommentThread
      comment={activeThread}
      replies={comments.replies(activeThread.id)}
      active={true}
      onResolve={handleResolve}
      onReopen={handleReopen}
      onReply={handleReply}
      onClose={() => {
        comments.activeId = null;
      }}
      nav={{ index: activeIndex, total: orderedThreads.length, onPrev: goToPrev, onNext: goToNext }}
      onAnchor={(o) => (threadAnchor = o)}
      fuzzy={comments.anchorStrategies.get(activeThread.id) === "fuzzy"}
    />
  </div>
{/if}
