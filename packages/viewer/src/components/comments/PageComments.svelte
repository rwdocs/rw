<script lang="ts">
  import type { Selector } from "../../types/comments";
  import { getRwContext } from "$lib/context";
  import CommentThread from "./CommentThread.svelte";
  import CommentForm from "./CommentForm.svelte";
  import Alert from "$lib/ui/primitives/Alert.svelte";
  import Badge from "$lib/ui/primitives/Badge.svelte";

  const { comments, page } = getRwContext();

  // Unique per instance so two embedded viewers on one page don't collide on
  // the heading id that labels the section (matches Popover/Menu primitives).
  const headingId = $props.id();

  const visibleThreads = $derived(
    comments.pageThreads
      .filter((t) => t.status !== "resolved")
      .toSorted((a, b) => a.createdAt.localeCompare(b.createdAt)),
  );

  const hasThreads = $derived(visibleThreads.length > 0);
  const countLabel = $derived(
    `${visibleThreads.length} ${visibleThreads.length === 1 ? "comment" : "comments"}`,
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

  {#if hasThreads}
    <div class="mb-6 space-y-4">
      {#each visibleThreads as thread (thread.id)}
        {@const quote = thread.selectors.length > 0 ? findQuote(thread.selectors) : null}
        <CommentThread
          comment={thread}
          {quote}
          replies={comments.replies(thread.id)}
          active={false}
          onResolve={handleResolve}
          onReopen={handleReopen}
          onReply={handleReply}
          onDelete={handleDelete}
          onRestore={handleRestore}
        />
      {/each}
    </div>
  {/if}

  <CommentForm onSubmit={handleNewComment} placeholder="Write a comment..." />
</section>
