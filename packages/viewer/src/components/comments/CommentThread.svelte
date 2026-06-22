<script lang="ts">
  import { untrack } from "svelte";
  import type { Author, Comment } from "../../types/comments";
  import Avatar from "$lib/ui/primitives/Avatar.svelte";
  import Badge from "$lib/ui/primitives/Badge.svelte";
  import Button from "$lib/ui/primitives/Button.svelte";
  import Quote from "$lib/ui/primitives/Quote.svelte";
  import { formatRelativeTime } from "$lib/ui/hooks/formatRelativeTime";
  import { useElementSize } from "$lib/ui/hooks/useElementSize.svelte";
  import CommentForm from "./CommentForm.svelte";
  import { buildCommentHash } from "$lib/comments/deeplink";

  function avatarVariant(author: Author): "person" | "ai" | "initials" {
    if (author.id === "local:ai") return "ai";
    if (author.id === "local:human") return "person";
    return "initials";
  }

  interface Props {
    comment: Comment;
    replies: Comment[];
    active: boolean;
    onResolve: (id: string) => void;
    onReopen: (id: string) => void;
    onReply: (parentId: string, body: string) => Promise<void>;
    onDelete: (id: string) => Promise<void>;
    onRestore: (id: string) => Promise<void>;
    onClose?: () => void;
    /** Navigation between threads (optional). */
    nav?: { index: number; total: number; onPrev: () => void; onNext: () => void };
    /** Called with the vertical distance from the thread's outer border to the
     *  avatar row's vertical center, whenever that distance changes. */
    onAnchor?: (offsetPx: number) => void;
    /** True when the comment was anchored via fuzzy matching — the original
     *  passage no longer appears verbatim, so the highlight may be approximate. */
    fuzzy?: boolean;
    /** Quote of the original passage, shown between the author row and the
     *  comment body. Populated for orphaned page comments and for resolved
     *  threads surfaced in the page-comments block (inline or page-level).
     *  Absent for open inline threads that are actively anchored in the
     *  document and for native page comments. */
    quote?: { prefix?: string; exact: string; suffix?: string } | null;
    /** `title` attribute for the rendered quote. Defaults to the orphaned-page
     *  message; the resolved-comments list overrides it with a neutral string
     *  because a resolved thread's passage usually still appears on the page. */
    quoteTitle?: string;
    /** DOM id for the card (e.g. `comment-<uuid>`) so a deep link can scroll to
     *  it; when set the card also becomes focusable (`tabindex="-1"`). */
    domId?: string;
    /** Value for `data-thread-id`, the scroll target keyboard navigation uses. */
    threadId?: string;
    /** Amber tint marking the current page comment — the active one (deep-link
     *  landing or keyboard-nav target), so the marker follows n/p navigation. */
    linked?: boolean;
    /** Initial reply-draft text for this thread, read once when the component
     *  mounts (e.g. the parent surface's per-thread draft from the comments
     *  store). Later changes to this prop are ignored — the live draft lives in
     *  local state and is reported back via `onReplyDraftChange`. */
    initialReplyDraft?: string;
    /** Called whenever this thread's reply draft changes, with this thread's id
     *  and the new body, so the parent surface can persist it (keyed by thread
     *  id). */
    onReplyDraftChange?: (threadId: string, body: string) => void;
  }

  let {
    comment,
    replies,
    active,
    onResolve,
    onReopen,
    onReply,
    onDelete,
    onRestore,
    onClose,
    nav,
    onAnchor,
    fuzzy = false,
    quote = null,
    quoteTitle = "This comment was attached to a passage that no longer appears on the page.",
    domId,
    threadId,
    linked = false,
    initialReplyDraft = "",
    onReplyDraftChange,
  }: Props = $props();

  let copied = $state(false);

  // Reply draft buffered in local state: seeded once from initialReplyDraft,
  // then owned here and reported back via onReplyDraftChange. The id is captured
  // once so the write always targets this thread. Each surface gives a thread
  // its own instance (the sidebar remounts via {#key}, the page list keys its
  // {#each}), so writing the live draft straight into the parent's store member
  // instead would let a thread switch flush this draft into the next slot.
  const draftThreadId = untrack(() => comment.id);
  let replyDraft = $state(untrack(() => initialReplyDraft));
  $effect(() => {
    onReplyDraftChange?.(draftThreadId, replyDraft);
  });

  async function copyLink() {
    const url = `${window.location.origin}${window.location.pathname}#${buildCommentHash(comment.id)}`;
    try {
      await navigator.clipboard.writeText(url);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      // Clipboard blocked (insecure context / permissions) — no-op.
    }
  }

  let outerRef: HTMLDivElement | undefined = $state();
  let avatarRowRef: HTMLDivElement | undefined = $state();

  const outerSize = useElementSize(() => outerRef ?? null);
  const rowSize = useElementSize(() => avatarRowRef ?? null);

  async function handleReply(body: string) {
    await onReply(comment.id, body);
  }

  let lastReported: number | null = null;
  $effect(() => {
    if (!onAnchor || !outerRef || !avatarRowRef) return;
    void outerSize.version;
    void rowSize.version;
    // Bounding rects rather than offsetTop because the outer div isn't
    // positioned, so its offsetParent isn't guaranteed to be the thread root.
    const outerRect = outerRef.getBoundingClientRect();
    const rowRect = avatarRowRef.getBoundingClientRect();
    const offset = rowRect.top - outerRect.top + rowRect.height / 2;
    if (lastReported === null || Math.abs(offset - lastReported) > 0.5) {
      lastReported = offset;
      onAnchor(offset);
    }
  });
</script>

<!-- tabindex is -1 (or absent): a deep link focuses the card programmatically;
     it is never placed in the tab order. -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div
  bind:this={outerRef}
  data-testid="comment-thread"
  id={domId}
  tabindex={domId != null ? -1 : undefined}
  data-thread-id={threadId}
  data-linked={linked ? "true" : undefined}
  class="
    thread-card overflow-hidden rounded-md border px-3 pt-3 transition-colors
    {active
    ? 'border-gray-200 bg-white dark:border-neutral-700 dark:bg-neutral-800'
    : `
      border-gray-200 bg-white
      hover:border-gray-300
      dark:border-neutral-700 dark:bg-neutral-800
      dark:hover:border-neutral-600
    `}
  "
>
  {#snippet closeButton()}
    <Button variant="ghost" size="xs" iconOnly onclick={onClose} aria-label="Close comment">
      <svg class="size-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
        <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
      </svg>
    </Button>
  {/snippet}

  <!-- Thread navigation -->
  {#if onClose}
    {#if nav && nav.total > 1}
      <div
        class="
          -mx-3 -mt-3 mb-2 flex items-center justify-between border-b border-gray-200 px-3 py-2
          dark:border-neutral-700
        "
      >
        <Badge intent="neutral" size="sm">{nav.index + 1} / {nav.total}</Badge>
        <div class="flex gap-1">
          <Button
            variant="ghost"
            size="xs"
            iconOnly
            disabled={nav.index <= 0}
            onclick={nav.onPrev}
            aria-label="Previous comment"
          >
            <svg
              class="size-4"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              stroke-width="2"
            >
              <path stroke-linecap="round" stroke-linejoin="round" d="M5 15l7-7 7 7" />
            </svg>
          </Button>
          <Button
            variant="ghost"
            size="xs"
            iconOnly
            disabled={nav.index >= nav.total - 1}
            onclick={nav.onNext}
            aria-label="Next comment"
          >
            <svg
              class="size-4"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              stroke-width="2"
            >
              <path stroke-linecap="round" stroke-linejoin="round" d="M19 9l-7 7-7-7" />
            </svg>
          </Button>
          {@render closeButton()}
        </div>
      </div>
    {:else}
      <div
        class="
          -mx-3 -mt-3 mb-2 flex justify-end border-b border-gray-200 px-3 py-2
          dark:border-neutral-700
        "
      >
        {@render closeButton()}
      </div>
    {/if}
  {/if}

  <!-- Main comment -->
  <div
    class={{
      "opacity-60": comment.status === "resolved",
    }}
  >
    <div
      bind:this={avatarRowRef}
      data-testid="comment-avatar-row"
      class="mb-2 flex items-center gap-2"
    >
      <Avatar
        variant={avatarVariant(comment.author)}
        name={comment.author.name}
        src={comment.author.avatarUrl}
        size={24}
      />
      <span class="text-sm font-semibold text-gray-900 dark:text-neutral-100">
        {comment.author.name}
      </span>
      {#if fuzzy}
        <Badge
          intent="warning"
          size="sm"
          class="italic"
          title="The exact passage this comment was attached to no longer appears in the page. The highlight is the closest match."
        >
          re-anchored
        </Badge>
      {/if}
      <div class="ml-auto flex items-center gap-2">
        <button
          type="button"
          onclick={copyLink}
          aria-label="Copy link"
          title={copied ? "Copied" : "Copy link to this comment"}
          class="
            cursor-pointer text-gray-400 transition-colors
            hover:text-gray-700
            dark:text-neutral-500
            dark:hover:text-neutral-200
          "
        >
          {#if copied}
            <svg
              class="size-3.5"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              stroke-width="2"
            >
              <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
            </svg>
          {:else}
            <svg
              class="size-3.5"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              stroke-width="2"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                d="M13.828 10.172a4 4 0 010 5.656l-3 3a4 4 0 11-5.656-5.656l1.5-1.5m6.656-1.328a4 4 0 010-5.656l3-3a4 4 0 115.656 5.656l-1.5 1.5"
              />
            </svg>
          {/if}
        </button>
        <span class="text-xs text-gray-400 dark:text-neutral-500">
          {formatRelativeTime(new Date(comment.createdAt))}
        </span>
      </div>
    </div>
    {#if quote}
      <Quote
        data-testid="orphan-quote"
        title={quoteTitle}
        class="mb-2"
        prefix={quote.prefix}
        exact={quote.exact}
        suffix={quote.suffix}
      />
    {/if}
    <div
      data-testid="comment-body"
      class="comment-body text-sm text-gray-900 dark:text-neutral-100"
    >
      <!-- `!= null`, not truthy: an empty string is a body that rendered to
           nothing (show nothing); only a missing field — a backend that didn't
           render server-side — falls back to the plain-text body. -->
      {#if comment.bodyHtml != null}{@html comment.bodyHtml}{:else}{comment.body}{/if}
    </div>
    <div class="my-2 flex items-center gap-2">
      {#if comment.canResolve && comment.status === "open"}
        <button
          type="button"
          onclick={() => onResolve(comment.id)}
          class="
            cursor-pointer text-xs text-gray-500 transition-colors
            hover:text-gray-900
            dark:text-neutral-400
            dark:hover:text-neutral-100
          "
        >
          Resolve
        </button>
      {:else if comment.canResolve && comment.status === "resolved"}
        <button
          type="button"
          onclick={() => onReopen(comment.id)}
          class="
            cursor-pointer text-xs text-gray-500 transition-colors
            hover:text-gray-900
            dark:text-neutral-400
            dark:hover:text-neutral-100
          "
        >
          Reopen
        </button>
      {/if}
    </div>
  </div>

  <!-- Replies -->
  {#if replies.length > 0}
    <div
      class="
        -mx-3 divide-y divide-gray-200 border-t border-gray-200 bg-gray-50
        dark:divide-neutral-700 dark:border-neutral-700 dark:bg-neutral-900/50
      "
      class:opacity-60={comment.status === "resolved"}
    >
      {#each replies as reply (reply.id)}
        <div class="px-3 py-2 {reply.deletedAt != null ? 'opacity-60' : ''}">
          <div class="mb-1 flex items-center gap-2">
            <Avatar
              variant={avatarVariant(reply.author)}
              name={reply.author.name}
              src={reply.author.avatarUrl}
              size={20}
            />
            <span class="text-xs font-semibold text-gray-900 dark:text-neutral-100">
              {reply.author.name}
            </span>
            <span class="ml-auto text-xs text-gray-400 dark:text-neutral-500">
              {formatRelativeTime(new Date(reply.createdAt))}
            </span>
          </div>
          <div
            class="
              comment-body text-sm text-gray-900
              dark:text-neutral-100
              {reply.deletedAt != null ? 'line-through' : ''}
            "
          >
            {#if reply.bodyHtml != null}{@html reply.bodyHtml}{:else}{reply.body}{/if}
          </div>
          <div class="mt-1 flex items-center gap-2">
            {#if reply.deletedAt != null && reply.canRestore}
              <button
                type="button"
                onclick={() => onRestore(reply.id)}
                class="
                  cursor-pointer text-xs text-gray-500 transition-colors
                  hover:text-gray-900
                  dark:text-neutral-400
                  dark:hover:text-neutral-100
                "
              >
                Restore
              </button>
            {/if}
            {#if reply.deletedAt == null && reply.canDelete}
              <button
                type="button"
                onclick={() => onDelete(reply.id)}
                class="
                  cursor-pointer text-xs text-gray-500 transition-colors
                  hover:text-red-600
                  dark:text-neutral-400
                  dark:hover:text-red-400
                "
              >
                Delete
              </button>
            {/if}
          </div>
        </div>
      {/each}
    </div>
  {/if}

  <!-- Reply form -->
  {#if comment.status === "open"}
    <div
      class="
        -mx-3 border-t border-gray-200 bg-gray-50 px-3 py-2
        dark:border-neutral-700 dark:bg-neutral-900/50
      "
    >
      <CommentForm bind:value={replyDraft} onSubmit={handleReply} placeholder="Write a reply..." />
    </div>
  {/if}
</div>

<style>
  /* Clear the sticky header when a deep link scrolls this card into view (same
     offset headings use, inherited from .layout-root). outline:none because the
     card is focused programmatically on landing; the tint below is the marker. */
  .thread-card {
    scroll-margin-top: var(--scroll-anchor-offset, 1.5rem);
    outline: none;
  }
  /* Persistent tint while this thread is the active deep-link target. */
  .thread-card[data-linked="true"] {
    box-shadow: 0 0 0 2px rgb(250 204 21 / 0.9);
  }

  .comment-body :global(p) {
    margin: 0;
  }
  .comment-body :global(p + p),
  .comment-body :global(ul + p),
  .comment-body :global(ol + p),
  .comment-body :global(blockquote + p),
  .comment-body :global(pre + p),
  .comment-body :global(ul),
  .comment-body :global(ol),
  .comment-body :global(blockquote),
  .comment-body :global(pre) {
    margin-top: 0.5rem;
  }
  .comment-body :global(ul),
  .comment-body :global(ol) {
    padding-left: 1.25rem;
  }
  .comment-body :global(ul) {
    list-style: disc;
  }
  .comment-body :global(ol) {
    list-style: decimal;
  }
  .comment-body :global(pre) {
    overflow-x: auto;
    padding: 0.5rem;
    border-radius: 0.25rem;
    /* Neutral translucent gray reads on both light and dark card backgrounds. */
    background: rgb(128 128 128 / 0.15);
  }
  .comment-body :global(code) {
    overflow-wrap: anywhere;
  }
  .comment-body :global(blockquote) {
    padding-left: 0.75rem;
    border-left: 2px solid currentColor;
    opacity: 0.85;
  }
  .comment-body :global(a) {
    text-decoration: underline;
  }
</style>
