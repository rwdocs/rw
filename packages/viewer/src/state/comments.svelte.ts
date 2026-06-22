import type { AnchorStrategy } from "$lib/anchoring";
import type { CommentApiClient } from "../api/comments";
import type { Comment, CreateCommentRequest, Selector } from "../types/comments";
import { resolveNavTarget, sortByOrder } from "$lib/comments/navigation";
import type { NotifyFn } from "../types/notify";

/** A new comment being drafted — selectors are captured, awaiting body text. */
export interface PendingComment {
  documentId: string;
  selectors: Selector[];
}

export class Comments {
  /** Whether the backend supports comments. */
  enabled = $state(false);
  items = $state.raw<Comment[]>([]);
  loading = $state(false);
  activeId = $state<string | null>(null);
  /** Vertical offset of the active highlight relative to the content area. */
  activeTop = $state<number | null>(null);
  /** Comment currently targeted by a `#comment-<id>` deep link. Owned by
   *  PageContent's inbound effect (sets/clears it as the hash moves); also reset
   *  to null by load() and clear() on document change. Drives the page-thread
   *  tint in PageComments. */
  linkedId = $state<string | null>(null);
  /** Open/closed state of the page-comments "Show resolved" disclosure. Set true
   *  by the inbound deep-link effect when a resolved thread is the target, and
   *  toggled by the disclosure button; reset on navigation. */
  resolvedExpanded = $state(false);
  /** Inline thread ids in current document order (by resolved DOM Range).
   *  Written by PageContent whenever highlights re-anchor; consumed by the
   *  sidebar to order prev/next navigation. Stored positions are stale when
   *  the document has been edited between comment creations, so ordering
   *  must come from the live DOM. */
  order = $state.raw<string[]>([]);
  /** Per-comment anchor strategy from the most recent re-anchor pass.
   *  Comments anchored via 'fuzzy' get a "re-anchored" indicator in the UI. */
  anchorStrategies = $state.raw<Map<string, AnchorStrategy>>(new Map());
  /** Ids of inline comments whose stored selectors no longer anchor to any
   *  text in the current document. The viewer surfaces these in the page
   *  comments timeline below the article (with their stored quote as context)
   *  instead of silently hiding them.
   *  Written by PageContent after each re-anchor pass. */
  orphanIds = $state.raw<Set<string>>(new Set());
  /** New comment being drafted (shown in sidebar). */
  pending = $state<PendingComment | null>(null);
  /** Vertical offset for the pending comment form. */
  pendingTop = $state<number | null>(null);
  /** Per-thread reply drafts, keyed by thread (top-level comment) id. Lives on
   *  the store so a draft survives the inline sidebar remounting CommentThread
   *  (and its CommentForm) on every thread switch, and so the same thread shows
   *  the same draft on whichever surface renders it. Reset on document change. */
  replyDrafts = $state<Record<string, string>>({});
  /** Bumped on every programmatic comment navigation (n/p). Rendering
   *  components watch it to scroll the now-active comment into view; a plain
   *  `activeId` change (e.g. clicking a highlight) must not trigger that
   *  scroll, so the bare id isn't enough of a signal.
   *
   *  Strictly monotonic for the lifetime of the instance — never reset (not
   *  even by `clear()`). Consumers detect a navigation by comparing against the
   *  last value they handled; resetting this counter could make a new value
   *  collide with a stale "last handled" value and silently skip one scroll. */
  navSeq = $state(0);
  /** Bumped on every `r` (focus-reply) request. The two comment render
   *  surfaces watch it to move focus into the active thread's reply box; a bare
   *  `activeId` change must not move focus, so — like `navSeq` — the counter is
   *  the signal. Strictly monotonic; never reset (not even by `clear()`). */
  replyFocusSeq = $state(0);

  private apiClient: CommentApiClient;
  private abortController: AbortController | null = null;
  private documentId: string | null = null;
  private notify: NotifyFn;

  constructor(apiClient: CommentApiClient, notify: NotifyFn) {
    this.apiClient = apiClient;
    this.notify = notify;
  }

  /** True when the underlying client provides its own live-refresh transport. */
  get canSubscribe(): boolean {
    return typeof this.apiClient.subscribe === "function";
  }

  /** Subscribe to live comment changes for `documentId` via the client's own
   *  transport. Call only when `canSubscribe` is true; returns the client's
   *  unsubscribe handle (safe to return directly from a Svelte `$effect`). The
   *  `| undefined` covers the no-`subscribe` client, where callers use the
   *  live-reload WebSocket instead. */
  subscribe(documentId: string, onChange: () => void): (() => void) | undefined {
    return this.apiClient.subscribe?.(documentId, onChange);
  }

  load = async (documentId: string, opts?: { silent?: boolean }) => {
    if (!this.enabled) return;
    if (this.abortController) {
      this.abortController.abort();
    }
    this.abortController = new AbortController();
    const signal = this.abortController.signal;

    if (documentId !== this.documentId) {
      this.activeId = null;
      this.linkedId = null;
      this.resolvedExpanded = false;
      this.clearPending();
      this.replyDrafts = {};
      this.documentId = documentId;
    }
    const silent = opts?.silent ?? false;
    if (!silent) {
      this.loading = true;
    }
    try {
      const items = await this.apiClient.list(documentId, { signal });
      if (signal.aborted) return;
      this.items = items;
    } catch (e) {
      if (e instanceof DOMException && e.name === "AbortError") return;
      if (silent) {
        // Silent (live-reload/subscribe) refresh failed: keep the rendered
        // comments and do not raise a toast the user never triggered. A
        // transient blip is recovered on the next successful reload.
        if (import.meta.env.DEV) {
          console.warn("[rw] silent comments refresh failed; keeping current comments:", e);
        }
        return;
      }
      this.notify({
        intent: "error",
        message: e instanceof Error ? e.message : "Failed to load comments",
      });
      this.items = [];
    } finally {
      // Clear even when silent: a silent winner that aborted a non-silent
      // in-flight load must still clear `loading`. The identity check ensures
      // only the current (non-superseded) invocation touches shared state.
      if (this.abortController?.signal === signal) {
        this.abortController = null;
        this.loading = false;
      }
    }
  };

  create = async (input: CreateCommentRequest) => {
    const comment = await this.apiClient.create(input);
    if (this.documentId === comment.documentId) {
      // Append the new row directly: every per-row capability flag depends only
      // on this row's own status + parentId + deletedAt, so no sibling needs
      // re-projection.
      this.items = [...this.items, comment];
    }
    // If response.documentId !== this.documentId, the user navigated away;
    // don't touch this.items to avoid polluting the new document's view.
    return comment;
  };

  resolve = async (id: string) => {
    const updated = await this.apiClient.update(id, { status: "resolved" });
    if (this.documentId === updated.documentId) {
      this.items = this.items.map((c) => (c.id === id ? updated : c));
    }
    // If response.documentId !== this.documentId, the user navigated away;
    // don't touch this.items to avoid polluting the new document's view.
  };

  reopen = async (id: string) => {
    const updated = await this.apiClient.update(id, { status: "open" });
    if (this.documentId === updated.documentId) {
      this.items = this.items.map((c) => (c.id === id ? updated : c));
    }
    // If response.documentId !== this.documentId, the user navigated away;
    // don't touch this.items to avoid polluting the new document's view.
  };

  delete = async (id: string) => {
    const deleted = await this.apiClient.delete(id);
    if (this.documentId === deleted.documentId) {
      // Replace the row with its deleted projection. Keep it visible in-session
      // so the user can restore it; reload will hide it (server filters deleted).
      this.items = this.items.map((c) => (c.id === id ? deleted : c));
    }
    // If response.documentId !== this.documentId, the user navigated away;
    // don't touch this.items to avoid polluting the new document's view.
  };

  restore = async (id: string) => {
    const restored = await this.apiClient.update(id, { status: "open" });
    if (this.documentId === restored.documentId) {
      this.items = this.items.map((c) => (c.id === id ? restored : c));
    }
    // If response.documentId !== this.documentId, the user navigated away;
    // don't touch this.items to avoid polluting the new document's view.
  };

  get threads(): Comment[] {
    return this.items.filter((c) => !c.parentId);
  }

  get inlineThreads(): Comment[] {
    return this.items.filter(
      (c) => !c.parentId && c.selectors.length > 0 && !this.orphanIds.has(c.id),
    );
  }

  /** Top-level threads shown in the page comments timeline below the article.
   *  Includes native page comments (no selectors) and orphaned inline comments
   *  whose stored selectors no longer anchor. */
  get pageThreads(): Comment[] {
    return this.items.filter(
      (c) => !c.parentId && (c.selectors.length === 0 || this.orphanIds.has(c.id)),
    );
  }

  /** True when the active thread is an inline (anchored) thread — the only case
   *  that should show the right-margin comment sidebar. Page/orphaned comments
   *  can also become `activeId` (keyboard navigation targets them), but they are
   *  shown in the bottom timeline, not the sidebar. */
  get activeIsInline(): boolean {
    return this.activeId != null && this.inlineThreads.some((t) => t.id === this.activeId);
  }

  /** All open top-level threads in review order: inline threads in document
   *  order (live DOM rank from `order`) followed by page-level + orphaned
   *  threads by creation time — matching the order `PageComments` renders them.
   *  Resolved threads are excluded. */
  get navigable(): string[] {
    const inline = sortByOrder(
      this.inlineThreads.filter((t) => t.status !== "resolved"),
      this.order,
    ).map((t) => t.id);
    const page = this.pageThreads
      .filter((t) => t.status !== "resolved")
      .toSorted((a, b) => a.createdAt.localeCompare(b.createdAt))
      .map((t) => t.id);
    return [...inline, ...page];
  }

  /** Move the active comment one step (with wrap-around), or enter from idle
   *  (next → first, prev → last). Returns the new position for announcement, or
   *  null when there are no navigable comments.
   *
   *  An arrow-function field (like `load`/`create`/`resolve` above) so `this`
   *  stays bound when it is passed as a callback — e.g. Layout hands
   *  `comments.navigate` to the keyboard hook. Converting it to a method would
   *  break that call site. */
  navigate = (
    direction: "next" | "prev",
  ): { index: number; total: number; author: string } | null => {
    const list = this.navigable;
    const target = resolveNavTarget(list, this.activeId, direction);
    if (target == null) return null;
    this.activeId = target;
    this.navSeq++;
    const author = this.items.find((c) => c.id === target)?.author.name ?? "";
    return { index: list.indexOf(target), total: list.length, author };
  };

  /** Request that the active thread's reply box take keyboard focus (the `r`
   *  shortcut). Returns the active thread's position for announcement, or null
   *  when there is nothing to reply to: a pending new comment is being drafted,
   *  no thread is active, or the active thread is missing or not open (resolved
   *  threads have no reply form). Bumps `replyFocusSeq` only on success.
   *
   *  An arrow-function field (like `navigate`) so `this` stays bound when Layout
   *  hands `comments.focusReply` to the keyboard hook. */
  focusReply = (): { index: number; total: number; author: string } | null => {
    if (this.pending != null) return null;
    const id = this.activeId;
    if (id == null) return null;
    const active = this.items.find((c) => c.id === id);
    if (!active || active.status !== "open") return null;
    this.replyFocusSeq++;
    const list = this.navigable;
    return { index: list.indexOf(id), total: list.length, author: active.author.name };
  };

  replies(parentId: string): Comment[] {
    return this.items.filter((c) => c.parentId === parentId);
  }

  clearPending = () => {
    this.pending = null;
    this.pendingTop = null;
  };

  /** Persist a thread's reply draft (keyed by thread id). An empty body deletes
   *  the entry instead of storing "", so replyDrafts never accumulates empty
   *  slots from a freshly-seeded or just-submitted thread. Arrow field so `this`
   *  stays bound when passed as a callback. */
  setReplyDraft = (threadId: string, body: string) => {
    if (body) {
      this.replyDrafts[threadId] = body;
    } else {
      delete this.replyDrafts[threadId];
    }
  };

  clear = () => {
    // Abort any in-flight load so a list() resolving after clear() hits load()'s
    // `signal.aborted` guard instead of repopulating the just-cleared list with
    // the previous document's comments — e.g. when navigating to a page that
    // shows no comments and never re-triggers a load.
    this.abortController?.abort();
    this.abortController = null;
    this.items = [];
    this.loading = false;
    this.activeId = null;
    this.linkedId = null;
    this.resolvedExpanded = false;
    this.activeTop = null;
    // navSeq is intentionally NOT reset here — see its declaration. It must stay
    // monotonic so a navigation after clear() can never collide with a value a
    // consumer already handled.
    this.order = [];
    this.anchorStrategies = new Map();
    this.orphanIds = new Set();
    this.documentId = null;
    this.replyDrafts = {};
    this.clearPending();
  };
}
