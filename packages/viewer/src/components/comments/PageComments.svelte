<script lang="ts">
  import type { Comment, Selector } from "../../types/comments";
  import { tick } from "svelte";
  import { getRwContext } from "$lib/context";
  import CommentThread from "./CommentThread.svelte";
  import CommentForm from "./CommentForm.svelte";
  import Badge from "$lib/ui/primitives/Badge.svelte";
  import Chevron from "$lib/ui/primitives/Chevron.svelte";
  import { buildCommentHash } from "$lib/comments/deeplink";
  import { restoreFocusToThread, focusReplyTextarea } from "$lib/comments/focus";
  import { documentIdFor } from "$lib/comments/documentId";
  import { useSeqChange } from "$lib/ui/hooks/useSeqChange.svelte";
  import { createCommentActions } from "$lib/comments/actions";

  const { comments, page, notify } = getRwContext();
  const actions = createCommentActions(comments, notify);

  // Unique per instance so two embedded viewers on one page don't collide on
  // the heading id that labels the section and the resolved-list aria-controls target.
  const headingId = $props.id();

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

  /** The active thread's card, or null when the active comment is an inline
   *  highlight (PageContent and the sidebar own those), not one of these cards. */
  function activeCard(): HTMLElement | null {
    const activeId = comments.activeId;
    if (!activeId || !visibleThreads.some((t) => t.id === activeId)) return null;
    return document.getElementById(buildCommentHash(activeId));
  }

  // Scroll the active card into view on keyboard navigation. Top-aligned: a card
  // holds the root comment plus every reply, so centering one taller than the
  // viewport would push the root off-screen above.
  useSeqChange(
    () => comments.navSeq,
    () => activeCard()?.scrollIntoView({ behavior: "auto", block: "start" }),
  );

  // Move focus into the active card's reply box on `r`.
  useSeqChange(
    () => comments.replyFocusSeq,
    () => focusReplyTextarea(activeCard()?.querySelector("textarea")),
    true,
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

  async function handleNewComment(body: string) {
    if (!page.data) return;
    const created = await actions.create({
      documentId: documentIdFor(page.data.meta),
      body,
      selectors: [],
    });
    // New comment succeeded: focus its freshly-rendered card so n/p navigation
    // resumes (the bottom form stays mounted and would otherwise keep focus).
    await tick();
    restoreFocusToThread(document.getElementById(buildCommentHash(created.id)));
  }
</script>

<section
  aria-labelledby={headingId}
  class="mt-12 border-t border-gray-200 pt-8 dark:border-neutral-700"
>
  <div class="mb-6 flex items-center gap-2">
    <h2 id={headingId} class="text-lg font-semibold text-gray-900 dark:text-neutral-100">
      Comments
    </h2>
    {#if hasThreads}
      <Badge intent="neutral" size="sm" label={countLabel}>
        {visibleThreads.length}
      </Badge>
    {/if}
  </div>

  {#snippet threadCard(thread: Comment, quoteTitle?: string)}
    {const quote = $derived(thread.selectors.length > 0 ? findQuote(thread.selectors) : null)}
    <!-- domId is the deep-link and keyboard-nav target; linked is the tint. -->
    <CommentThread
      comment={thread}
      {quote}
      {quoteTitle}
      domId={buildCommentHash(thread.id)}
      linked={thread.id === comments.activeId ||
        (comments.activeId == null && thread.id === comments.linkedId)}
      replies={comments.replies(thread.id)}
      onResolve={actions.resolve}
      onReopen={actions.reopen}
      onReply={actions.reply}
      onDelete={actions.remove}
      onRestore={actions.restore}
      initialReplyDraft={comments.replyDrafts[thread.id]}
      onReplyDraftChange={comments.setReplyDraft}
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
