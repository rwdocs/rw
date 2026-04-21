<script lang="ts">
  import type { Selector } from "../../types/comments";
  import { getRwContext } from "../../lib/context";
  import CommentThread from "./CommentThread.svelte";
  import CommentForm from "./CommentForm.svelte";

  const { comments, page } = getRwContext();

  const visibleThreads = $derived(
    comments.pageThreads
      .filter((t) => t.status !== "resolved")
      .toSorted((a, b) => a.createdAt.localeCompare(b.createdAt)),
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
  aria-label="Page comments"
  class="mt-12 border-t border-gray-200 pt-8 dark:border-neutral-700"
>
  {#if comments.error}
    <div
      class="
        mb-4 rounded-lg border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700
        dark:border-red-800 dark:bg-red-950 dark:text-red-300
      "
      role="alert"
    >
      {comments.error}
      <button
        class="ml-2 font-medium underline hover:no-underline"
        onclick={() => {
          comments.error = null;
        }}>Dismiss</button
      >
    </div>
  {/if}

  {#if visibleThreads.length > 0}
    <div class="mb-6 space-y-4">
      {#each visibleThreads as thread (thread.id)}
        {@const quote = thread.selectors.length > 0 ? findQuote(thread.selectors) : null}
        <div>
          {#if quote}
            <!-- Orphaned inline comment: the stored passage no longer appears
                 in the page, so we show the quote (with its captured context)
                 verbatim above the thread. -->
            <blockquote
              data-testid="orphan-quote"
              title="This comment was attached to a passage that no longer appears on the page."
              class="
                mb-2 border-l-2 border-amber-300 pl-3 text-sm text-gray-600 italic
                dark:border-amber-500/60 dark:text-neutral-400
              "
            >
              {#if quote.prefix}<span class="opacity-70">…{quote.prefix}</span>{/if}<mark
                class="
                  rounded-sm bg-[rgba(255,212,0,0.4)] px-0.5 text-inherit not-italic
                  dark:bg-[rgba(255,212,0,0.25)]
                ">{quote.exact}</mark
              >{#if quote.suffix}<span class="opacity-70">{quote.suffix}…</span>{/if}
            </blockquote>
          {/if}
          <CommentThread
            comment={thread}
            replies={comments.replies(thread.id)}
            active={false}
            onResolve={handleResolve}
            onReopen={handleReopen}
            onReply={handleReply}
          />
        </div>
      {/each}
    </div>
  {/if}

  <CommentForm onSubmit={handleNewComment} placeholder="Write a comment..." />
</section>
