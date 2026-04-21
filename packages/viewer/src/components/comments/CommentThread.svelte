<script lang="ts">
  import type { Comment } from "../../types/comments";
  import Avatar from "./Avatar.svelte";
  import CommentForm from "./CommentForm.svelte";

  interface Props {
    comment: Comment;
    replies: Comment[];
    active: boolean;
    onResolve: (id: string) => void;
    onReopen: (id: string) => void;
    onReply: (parentId: string, body: string) => Promise<void>;
    onClose?: () => void;
    /** Navigation between threads (optional). */
    nav?: { index: number; total: number; onPrev: () => void; onNext: () => void };
    /** Called with the vertical distance from the thread's outer border to the
     *  avatar row's vertical center, whenever that distance changes. */
    onAnchor?: (offsetPx: number) => void;
    /** True when the comment was anchored via fuzzy matching — the original
     *  passage no longer appears verbatim, so the highlight may be approximate. */
    fuzzy?: boolean;
  }

  let {
    comment,
    replies,
    active,
    onResolve,
    onReopen,
    onReply,
    onClose,
    nav,
    onAnchor,
    fuzzy = false,
  }: Props = $props();

  let outerRef: HTMLDivElement | undefined = $state();
  let avatarRowRef: HTMLDivElement | undefined = $state();

  function formatRelativeTime(dateStr: string): string {
    const date = new Date(dateStr);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffSec = Math.floor(diffMs / 1000);
    const diffMin = Math.floor(diffSec / 60);
    const diffHr = Math.floor(diffMin / 60);
    const diffDay = Math.floor(diffHr / 24);

    if (diffSec < 60) return "just now";
    if (diffMin < 60) return `${diffMin}m ago`;
    if (diffHr < 24) return `${diffHr}h ago`;
    if (diffDay < 30) return `${diffDay}d ago`;
    return date.toLocaleDateString();
  }

  async function handleReply(body: string) {
    await onReply(comment.id, body);
  }

  $effect(() => {
    if (!onAnchor || !outerRef || !avatarRowRef) return;

    let lastReported: number | null = null;
    const measure = () => {
      if (!outerRef || !avatarRowRef) return;
      // offsetTop is relative to offsetParent, which isn't guaranteed to be
      // outerRef (outer div isn't positioned). Use bounding rects instead so
      // the returned offset is always relative to the thread's outer border.
      const outerRect = outerRef.getBoundingClientRect();
      const rowRect = avatarRowRef.getBoundingClientRect();
      const offset = rowRect.top - outerRect.top + rowRect.height / 2;
      if (lastReported === null || Math.abs(offset - lastReported) > 0.5) {
        lastReported = offset;
        onAnchor?.(offset);
      }
    };

    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(outerRef);
    observer.observe(avatarRowRef);
    return () => observer.disconnect();
  });
</script>

<div
  bind:this={outerRef}
  class="
    overflow-hidden rounded-md border px-3 pt-3 transition-colors
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
  <!-- Thread navigation -->
  {#if onClose}
    {#if nav && nav.total > 1}
      <div
        class="
          -mx-3 -mt-3 mb-2 flex items-center justify-between border-b border-gray-200 px-3 py-2
          dark:border-neutral-700
        "
      >
        <span class="text-xs text-gray-500 dark:text-neutral-400">
          {nav.index + 1} / {nav.total}
        </span>
        <div class="flex gap-1">
          <button
            type="button"
            disabled={nav.index <= 0}
            onclick={nav.onPrev}
            aria-label="Previous comment"
            class="
              cursor-pointer rounded-sm p-0.5 text-gray-500 transition-colors
              hover:bg-gray-100 hover:text-gray-700
              disabled:cursor-default disabled:opacity-30
              disabled:hover:bg-transparent
              dark:text-neutral-400
              dark:hover:bg-neutral-700 dark:hover:text-neutral-200
            "
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
          </button>
          <button
            type="button"
            disabled={nav.index >= nav.total - 1}
            onclick={nav.onNext}
            aria-label="Next comment"
            class="
              cursor-pointer rounded-sm p-0.5 text-gray-500 transition-colors
              hover:bg-gray-100 hover:text-gray-700
              disabled:cursor-default disabled:opacity-30
              disabled:hover:bg-transparent
              dark:text-neutral-400
              dark:hover:bg-neutral-700 dark:hover:text-neutral-200
            "
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
          </button>
          <button
            type="button"
            onclick={onClose}
            aria-label="Close comment"
            class="
              cursor-pointer rounded-sm p-0.5 text-gray-500 transition-colors
              hover:bg-gray-100 hover:text-gray-700
              dark:text-neutral-400
              dark:hover:bg-neutral-700 dark:hover:text-neutral-200
            "
          >
            <svg
              class="size-4"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              stroke-width="2"
            >
              <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
      </div>
    {:else}
      <div
        class="
          -mx-3 -mt-3 mb-2 flex justify-end border-b border-gray-200 px-3 py-2
          dark:border-neutral-700
        "
      >
        <button
          type="button"
          onclick={onClose}
          aria-label="Close comment"
          class="
            cursor-pointer rounded-sm p-0.5 text-gray-500 transition-colors
            hover:bg-gray-100 hover:text-gray-700
            dark:text-neutral-400
            dark:hover:bg-neutral-700 dark:hover:text-neutral-200
          "
        >
          <svg
            class="size-4"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            stroke-width="2"
          >
            <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>
    {/if}
  {/if}

  <!-- Main comment -->
  <div class={comment.status === "resolved" ? "opacity-60" : ""}>
    <div
      bind:this={avatarRowRef}
      data-testid="comment-avatar-row"
      class="mb-2 flex items-center gap-2"
    >
      <Avatar author={comment.author} size={24} />
      <span class="text-sm font-semibold text-gray-900 dark:text-neutral-100">
        {comment.author.name}
      </span>
      {#if fuzzy}
        <span
          class="text-xs text-amber-600 italic dark:text-amber-400"
          title="The exact passage this comment was attached to no longer appears in the page. The highlight is the closest match."
        >
          re-anchored
        </span>
      {/if}
      <span class="ml-auto text-xs text-gray-400 dark:text-neutral-500">
        {formatRelativeTime(comment.createdAt)}
      </span>
    </div>
    <p
      class="
        text-sm text-gray-900
        dark:text-neutral-100
        {comment.status === 'resolved' ? 'line-through' : ''}
      "
    >
      {comment.body}
    </p>
    <div class="my-2 flex items-center gap-2">
      {#if comment.status === "open"}
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
      {:else}
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
    >
      {#each replies as reply (reply.id)}
        <div class="px-3 py-2">
          <div class="mb-1 flex items-center gap-2">
            <Avatar author={reply.author} size={20} />
            <span class="text-xs font-semibold text-gray-900 dark:text-neutral-100">
              {reply.author.name}
            </span>
            <span class="ml-auto text-xs text-gray-400 dark:text-neutral-500">
              {formatRelativeTime(reply.createdAt)}
            </span>
          </div>
          <p class="text-sm text-gray-900 dark:text-neutral-100">
            {reply.body}
          </p>
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
      <CommentForm onSubmit={handleReply} placeholder="Write a reply..." />
    </div>
  {/if}
</div>
