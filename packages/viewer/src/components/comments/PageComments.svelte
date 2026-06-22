<script lang="ts">
  import type { Comment, Selector } from "../../types/comments";
  import { tick } from "svelte";
  import { getRwContext } from "$lib/context";
  import CommentThread from "./CommentThread.svelte";
  import CommentForm from "./CommentForm.svelte";
  import Badge from "$lib/ui/primitives/Badge.svelte";
  import Chevron from "$lib/ui/primitives/Chevron.svelte";
  import { buildCommentHash } from "$lib/comments/deeplink";
  import { restoreFocusToThread } from "$lib/comments/focus";
  import { documentIdFor } from "$lib/comments/documentId";
  import { escapeId } from "$lib/comments/highlight";
  import { useScrollIntoViewOnNav } from "$lib/ui/hooks/useScrollIntoViewOnNav.svelte";
  import { SAVE_FAILED_MESSAGE } from "$lib/comments/messages";

  const { comments, page, notify } = getRwContext();

  // Unique per instance so two embedded viewers on one page don't collide on
  // the heading id that labels the section and the resolved-list aria-controls target.
  const headingId = $props.id();

  // Captured for keyboard-nav scroll: useScrollIntoViewOnNav finds the active
  // page/orphaned comment card within this section.
  let sectionRef: HTMLElement | undefined = $state();
  const resolvedListId = `${headingId}-resolved`;

  // Resolved disclosure: driven by the shared store so a deep link to a resolved
  // thread can force it open (comments.resolvedExpanded). The toggle button
  // writes the same field.
  const resolvedToggleLabel = $derived(
    comments.resolvedExpanded ? "Hide resolved" : "Show resolved",
  );

  const byCreatedAt = (a: Comment, b: Comment) => a.createdAt.localeCompare(b.createdAt);

  // Sourced from `comments.threads` (all top-level), not `pageThreads`: resolved
  // inline-anchored threads are never flagged orphaned, so they never appear in
  // `pageThreads`. Both the open list and this resolved list need them.
  const resolvedThreads = $derived(
    comments.threads.filter((t) => t.status === "resolved").toSorted(byCreatedAt),
  );
  const hasResolved = $derived(resolvedThreads.length > 0);

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
  // Top-align ("start", not the default "center"): a thread card holds the root
  // comment plus every reply, so centering a card taller than the viewport
  // pushes the root off-screen above; top-aligning keeps the root visible.
  useScrollIntoViewOnNav(
    () => comments.navSeq,
    () => {
      const activeId = comments.activeId;
      if (!activeId || !visibleThreads.some((t) => t.id === activeId)) return null;
      return sectionRef?.querySelector(`[data-thread-id="${escapeId(activeId)}"]`);
    },
    "start",
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
      notify({
        intent: "error",
        message: e instanceof Error ? e.message : "Failed to resolve comment",
      });
    }
  }

  async function handleReopen(id: string) {
    try {
      await comments.reopen(id);
    } catch (e) {
      notify({
        intent: "error",
        message: e instanceof Error ? e.message : "Failed to reopen comment",
      });
    }
  }

  async function handleDelete(id: string) {
    try {
      await comments.delete(id);
    } catch (e) {
      notify({
        intent: "error",
        message: e instanceof Error ? e.message : "Failed to delete comment",
      });
    }
  }

  async function handleRestore(id: string) {
    try {
      await comments.restore(id);
    } catch (e) {
      notify({
        intent: "error",
        message: e instanceof Error ? e.message : "Failed to restore comment",
      });
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
      notify({ intent: "error", message: SAVE_FAILED_MESSAGE });
      throw e;
    }
    // Reply succeeded: focus the parent thread card so n/p navigation resumes.
    await tick();
    restoreFocusToThread(document.getElementById(buildCommentHash(parentId)));
  }

  async function handleNewComment(body: string) {
    if (!page.data) return;
    const documentId = documentIdFor(page.data.meta);
    let created: Comment;
    try {
      created = await comments.create({
        documentId,
        body,
        selectors: [],
      });
    } catch (e) {
      notify({ intent: "error", message: SAVE_FAILED_MESSAGE });
      throw e;
    }
    // New comment succeeded: focus its freshly-rendered card so n/p navigation
    // resumes (the bottom form stays mounted and would otherwise keep focus).
    await tick();
    restoreFocusToThread(document.getElementById(buildCommentHash(created.id)));
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

  {#snippet threadCard(thread: Comment, quoteTitle?: string)}
    {@const quote = thread.selectors.length > 0 ? findQuote(thread.selectors) : null}
    <!-- The card carries its own targeting: domId (deep-link scroll/focus),
         threadId (data-thread-id, keyboard-nav target), and linked (the tint). -->
    <CommentThread
      comment={thread}
      {quote}
      {quoteTitle}
      domId={buildCommentHash(thread.id)}
      threadId={thread.id}
      linked={thread.id === comments.activeId ||
        (comments.activeId == null && thread.id === comments.linkedId)}
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
        {@render threadCard(thread)}
      {/each}
    </div>
  {/if}

  <CommentForm onSubmit={handleNewComment} placeholder="Write a comment..." />

  {#if hasResolved}
    <div class="my-6">
      <button
        type="button"
        onclick={() => (comments.resolvedExpanded = !comments.resolvedExpanded)}
        aria-expanded={comments.resolvedExpanded}
        aria-controls={resolvedListId}
        class="
          flex cursor-pointer items-center gap-1 text-sm text-gray-500 transition-colors
          hover:text-gray-900
          dark:text-neutral-400
          dark:hover:text-neutral-100
        "
      >
        <Chevron
          direction={comments.resolvedExpanded ? "down" : "right"}
          size="md"
          class="transition-transform"
          aria-hidden="true"
        />
        {resolvedToggleLabel}
        <Badge intent="neutral" size="md">{resolvedThreads.length}</Badge>
      </button>

      <!-- id stays in the DOM whenever the toggle button is rendered — the
           button's aria-controls target must resolve even while collapsed; only
           the list contents below are gated on resolvedExpanded. -->
      <div id={resolvedListId} class="mt-4 space-y-4">
        {#if comments.resolvedExpanded}
          {#each resolvedThreads as thread (thread.id)}
            {@render threadCard(thread, "The passage this comment was attached to.")}
          {/each}
        {/if}
      </div>
    </div>
  {/if}
</section>
