<script lang="ts">
  import type { Comment, Selector } from "../../types/comments";
  import { getRwContext } from "$lib/context";
  import CommentThread from "./CommentThread.svelte";
  import CommentForm from "./CommentForm.svelte";
  import Alert from "$lib/ui/primitives/Alert.svelte";
  import Badge from "$lib/ui/primitives/Badge.svelte";
  import Chevron from "$lib/ui/primitives/Chevron.svelte";
  import { escapeId } from "$lib/comments/highlight";
  import { useScrollIntoViewOnNav } from "$lib/ui/hooks/useScrollIntoViewOnNav.svelte";

  const { comments, page } = getRwContext();

  // Unique per instance so two embedded viewers on one page don't collide on
  // the heading id that labels the section (matches Popover/Menu primitives).
  const headingId = $props.id();

  // Local-only UI state: the resolved comments are an occasional lookup, kept
  // collapsed by default. Sourced from `comments.threads` (all top-level,
  // open + resolved) rather than `pageThreads`, because resolved inline-anchored
  // threads are never flagged `orphaned` and so never appear in `pageThreads`.
  let showResolved = $state(false);
  let sectionRef: HTMLElement | undefined = $state();
  const resolvedListId = `${headingId}-resolved`;

  const byCreatedAt = (a: Comment, b: Comment) => a.createdAt.localeCompare(b.createdAt);

  const resolvedThreads = $derived(
    comments.threads.filter((t) => t.status === "resolved").toSorted(byCreatedAt),
  );
  const hasResolved = $derived(resolvedThreads.length > 0);
  const resolvedToggleLabel = $derived(showResolved ? "Hide resolved" : "Show resolved");

  const visibleThreads = $derived(
    comments.pageThreads.filter((t) => t.status !== "resolved").toSorted(byCreatedAt),
  );

  const hasThreads = $derived(visibleThreads.length > 0);
  const countLabel = $derived(
    `${visibleThreads.length} ${visibleThreads.length === 1 ? "comment" : "comments"}`,
  );

  // Scroll the active page/orphaned comment into view on keyboard navigation.
  // The findTarget thunk returns null when the active comment is an inline
  // highlight (handled by PageContent) rather than one of this section's cards.
  useScrollIntoViewOnNav(
    () => comments.navSeq,
    () => {
      const activeId = comments.activeId;
      if (!activeId || !visibleThreads.some((t) => t.id === activeId)) return null;
      return sectionRef?.querySelector(`[data-thread-id="${escapeId(activeId)}"]`);
    },
  );

  function findQuote(
    selectors: Selector[],
  ): Extract<Selector, { type: "TextQuoteSelector" }> | null {
    return (
      selectors.find(
        (s): s is Extract<Selector, { type: "TextQuoteSelector" }> =>
          s.type === "TextQuoteSelector",
      ) ?? null
    );
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

  async function handleDelete(id: string) {
    try {
      await comments.delete(id);
    } catch (e) {
      comments.error = e instanceof Error ? e.message : "Failed to delete comment";
    }
  }

  async function handleRestore(id: string) {
    try {
      await comments.restore(id);
    } catch (e) {
      comments.error = e instanceof Error ? e.message : "Failed to restore comment";
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

  async function handleNewComment(body: string) {
    if (!page.data) return;
    const documentId = page.data.meta.path.replace(/^\//, "");
    try {
      await comments.create({
        documentId,
        body,
        selectors: [],
      });
    } catch (e) {
      comments.error = e instanceof Error ? e.message : "Failed to create comment";
    }
  }
</script>

<section
  bind:this={sectionRef}
  aria-labelledby={headingId}
  class="mt-12 border-t border-gray-200 pt-8 dark:border-neutral-700"
>
  <div class="mb-6 flex items-center gap-2">
    <h2 id={headingId} class="text-lg font-semibold text-gray-900 dark:text-neutral-100">
      Comments
    </h2>
    {#if hasThreads}
      <Badge intent="neutral" size="sm" aria-label={countLabel}>
        {visibleThreads.length}
      </Badge>
    {/if}
  </div>

  {#if comments.error}
    <Alert intent="danger" dismissible onDismiss={() => (comments.error = null)} class="mb-4">
      {comments.error}
    </Alert>
  {/if}

  {#snippet threadCard(thread: Comment, quoteTitle?: string)}
    {@const quote = thread.selectors.length > 0 ? findQuote(thread.selectors) : null}
    <CommentThread
      comment={thread}
      {quote}
      {quoteTitle}
      replies={comments.replies(thread.id)}
      active={false}
      onResolve={handleResolve}
      onReopen={handleReopen}
      onReply={handleReply}
      onDelete={handleDelete}
      onRestore={handleRestore}
    />
  {/snippet}

  {#if hasThreads}
    <div class="mb-6 space-y-4">
      {#each visibleThreads as thread (thread.id)}
        <!-- Wrapper carries data-thread-id as the scroll target for keyboard nav. -->
        <div data-thread-id={thread.id}>
          {@render threadCard(thread)}
        </div>
      {/each}
    </div>
  {/if}

  <CommentForm onSubmit={handleNewComment} placeholder="Write a comment..." />

  {#if hasResolved}
    <div class="my-6">
      <button
        type="button"
        onclick={() => (showResolved = !showResolved)}
        aria-expanded={showResolved}
        aria-controls={resolvedListId}
        class="
          flex cursor-pointer items-center gap-1 text-sm text-gray-500 transition-colors
          hover:text-gray-900
          dark:text-neutral-400
          dark:hover:text-neutral-100
        "
      >
        <Chevron
          direction={showResolved ? "down" : "right"}
          size="md"
          class="transition-transform"
          aria-hidden="true"
        />
        {resolvedToggleLabel}
        <Badge intent="neutral" size="md">{resolvedThreads.length}</Badge>
      </button>

      <!-- Container stays in the DOM even when collapsed so the button's
           aria-controls target always resolves; only its contents are gated. -->
      <div id={resolvedListId} class="mt-4 space-y-4">
        {#if showResolved}
          {#each resolvedThreads as thread (thread.id)}
            {@render threadCard(thread, "The passage this comment was attached to.")}
          {/each}
        {/if}
      </div>
    </div>
  {/if}
</section>
