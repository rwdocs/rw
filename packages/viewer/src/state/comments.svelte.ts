import type { AnchorStrategy } from "$lib/anchoring";
import type { CommentApiClient } from "../api/comments";
import type { Comment, CreateCommentRequest, Selector } from "../types/comments";
import { resolveNavTarget, sortByOrder, holdsSlot } from "$lib/comments/navigation";
import type { NotifyFn } from "../types/notify";

/** A new comment being drafted — selectors are captured, awaiting body text. */
export interface PendingComment {
  documentId: string;
  selectors: Selector[];
}

/** Splice `row` into `items`, which it mutates, in the creation order
 *  `CommentApiClient.list` uses. Appending would drop a reply put back into a
 *  thread below the ones that follow it. */
function insertByCreatedAt(items: Comment[], row: Comment) {
  const at = items.findIndex((c) => c.createdAt > row.createdAt);
  items.splice(at === -1 ? items.length : at, 0, row);
}

export class Comments {
  /** Whether the backend supports comments. */
  enabled = $state(false);
  items = $state.raw<Comment[]>([]);
  loading = $state(false);
  activeId = $state<string | null>(null);
  /** Vertical offset of the active highlight relative to the content area. */
  activeTop = $state<number | null>(null);
  /** Article-relative left edge for the narrow-screen comment popover, clamped so
   *  the fixed-width popover stays on screen while centering on the active
   *  highlight. Null when no inline thread is active. Only the popover reads it;
   *  the wide sidebar ignores it. */
  activeLeft = $state<number | null>(null);
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
   *  Comments anchored via 'fuzzy' get a "fuzzy" indicator in the UI. */
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
  /** Article-relative left edge for the pending draft in the narrow popover
   *  (see {@link activeLeft}). Null when no draft is pending. */
  pendingLeft = $state<number | null>(null);
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
  /** Rows written locally since the in-flight load's request left, laid back
   *  over that load's answer so it cannot undo them. Cleared when a load starts:
   *  anything written before the request left is in the answer it will bring.
   *
   *  `mayAdd` is whether a list *omitting* the row tells us anything. It does
   *  not for a row this reader just created, deleted or restored — an older list
   *  predates the write. It does for a status change: the row is missing because
   *  someone else deleted it, and putting it back resurrects it.
   *
   *  Keep this a plain `Map`, here and in {@link tombstones}. `load` clears it
   *  inside the `$effect` that calls `load`, so a `SvelteMap` would re-trigger
   *  that effect. (`$state` is inert — it does not proxy a Map, which is why
   *  `navigation.svelte.ts` reaches for `SvelteSet`.) */
  private unseenWrites = new Map<string, { row: Comment; mayAdd: boolean }>();
  /** Rows this reader soft-deleted. The server filters them out of every list,
   *  so unlike {@link unseenWrites} they outlive the load that raced them, which
   *  is what keeps Restore reachable until the reader acts. An entry also goes
   *  when a list shows the row live again, whoever restored it — a list
   *  predating this reader's own delete says the same and means the opposite. */
  private tombstones = new Map<string, Comment>();
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

  /** Load the server's list for `documentId` and apply it — amended, because a
   *  response that left before this reader's last write would otherwise undo it.
   *  See {@link unseenWrites}. */
  load = async (documentId: string, opts?: { silent?: boolean }) => {
    const silent = opts?.silent ?? false;
    if (!this.enabled) return;
    if (this.abortController) {
      this.abortController.abort();
    }
    this.abortController = new AbortController();
    const signal = this.abortController.signal;

    if (documentId !== this.documentId) {
      // Reset `items` too: nothing downstream filters rows by documentId, so
      // until this document's list arrives the previous document's comments
      // would render beneath it and their quotes anchor into its text — and
      // stay there if this load then fails.
      this.items = [];
      this.tombstones.clear();
      this.activeId = null;
      this.linkedId = null;
      this.resolvedExpanded = false;
      this.clearPending();
      this.replyDrafts = {};
      this.documentId = documentId;
    }
    if (!silent) {
      this.loading = true;
    }
    this.unseenWrites.clear();
    try {
      const items = await this.apiClient.list(documentId, { signal });
      if (signal.aborted) return;
      this.items = this.withLocalWrites(items);
    } catch (e) {
      // Not every failure of a superseded load is an AbortError: an HTTP status
      // the client rejects on, or a host client that ignores the signal, arrives
      // as a plain Error and would otherwise blank the page the reader moved to.
      if (signal.aborted) return;
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
      // A write landed mid-flight, so `items` holds a change the server
      // accepted; blanking it would lose that.
      if (this.unseenWrites.size > 0) return;
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

  /** `items` as the server's list amended by this reader's own writes — see
   *  {@link unseenWrites} and {@link tombstones} for which rows those are and
   *  why. Also forgets any tombstone the list has brought back. */
  private withLocalWrites(items: Comment[]): Comment[] {
    if (this.unseenWrites.size === 0 && this.tombstones.size === 0) return items;
    const listed = new Set(items.map((c) => c.id));
    const merged = items.map((row) => this.unseenWrites.get(row.id)?.row ?? row);
    for (const [id, { row, mayAdd }] of this.unseenWrites) {
      if (mayAdd && !listed.has(id)) insertByCreatedAt(merged, row);
    }
    for (const [id, row] of this.tombstones) {
      // Skipped, not just already placed: this list's view of the row is older
      // than a write of our own, so it cannot be read as a restore below.
      if (this.unseenWrites.has(id)) continue;
      if (listed.has(id)) {
        // Someone restored it, so "this reader deleted it" has stopped being
        // true — and holding the record would delete the row again on screen the
        // next time anyone else does.
        this.tombstones.delete(id);
      } else {
        insertByCreatedAt(merged, row);
      }
    }
    return merged;
  }

  /** The single write path for every local mutation of `items`. A mutator that
   *  assigned `items` directly would be invisible to {@link unseenWrites}, and
   *  the next list to land would undo it.
   *
   *  A `row` whose documentId is not the one on screen was answered after the
   *  user navigated away, so nothing happens, not even the record. */
  private commit(row: Comment, next: (items: Comment[]) => Comment[]) {
    if (this.documentId !== row.documentId) return;
    const updated = next(this.items);
    // A reducer that hands back the array it was given changed nothing on
    // screen, so there is nothing to protect from the next list. Recording the
    // row anyway would lay this copy of it back over a list that may hold a
    // newer one, silently reverting another reader's edit.
    if (updated === this.items) return;
    // The three writes {@link unseenWrites} lets back into a list that omits the
    // row: a create, a restore, a delete.
    const previous = this.items.find((c) => c.id === row.id);
    const mayAdd = previous == null || previous.deletedAt != null || row.deletedAt != null;
    this.items = updated;
    this.unseenWrites.set(row.id, { row, mayAdd });
    if (row.deletedAt != null) {
      this.tombstones.set(row.id, row);
    } else {
      this.tombstones.delete(row.id);
    }
  }

  /** Swap the stored row for the projection a mutation returned. Assumes a
   *  row's capability flags derive from that row alone, so no sibling needs
   *  re-projecting — if that stops holding, replacing in place is not enough.
   *
   *  A row that has left the screen is not put back: the likeliest reason is a
   *  refresh dropping it because another reader deleted it. */
  private replaceRow(id: string, row: Comment) {
    this.commit(row, (items) => {
      const at = items.findIndex((c) => c.id === id);
      return at === -1 ? items : items.with(at, row);
    });
  }

  /** Like {@link replaceRow}, but puts the row back when a refresh has already
   *  taken it off screen. Sound only where the row itself says why a list would
   *  omit it, which a soft-deleted one does — for anything else the row is
   *  missing because another reader deleted it. */
  private putRow(row: Comment) {
    this.commit(row, (items) => {
      const at = items.findIndex((c) => c.id === row.id);
      if (at !== -1) return items.with(at, row);
      const next = [...items];
      insertByCreatedAt(next, row);
      return next;
    });
  }

  create = async (input: CreateCommentRequest) => {
    const comment = await this.apiClient.create(input);
    this.commit(comment, (items) =>
      // A refresh running concurrently with this POST can deliver the new row
      // before the POST's own response arrives, so the list may already hold
      // it; appending unconditionally would show one comment twice.
      items.some((c) => c.id === comment.id) ? items : [...items, comment],
    );
    return comment;
  };

  resolve = async (id: string) => {
    this.replaceRow(id, await this.apiClient.update(id, { status: "resolved" }));
  };

  reopen = async (id: string) => {
    this.replaceRow(id, await this.apiClient.update(id, { status: "open" }));
  };

  delete = async (id: string) => {
    // `putRow`: the refresh this delete triggers can reach the browser first and
    // take the row off screen before this answer arrives.
    this.putRow(await this.apiClient.delete(id));
  };

  restore = async (id: string) => {
    this.replaceRow(id, await this.apiClient.update(id, { status: "open" }));
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

  /** True when an inline thread is active or a new-comment draft is pending — i.e.
   *  the right-margin aside (wide) / CommentPopover (narrow) should be shown. The
   *  single source for that predicate, used by Layout's `data-comments-active`
   *  attribute, the aside `{#if}`, and the popover's `show`, so the three can't
   *  drift apart. */
  get inlineSurfaceActive(): boolean {
    return this.activeIsInline || this.pending != null;
  }

  /** All top-level threads in review order: inline threads in document order
   *  (live DOM rank from `order`) followed by page-level + orphaned threads by
   *  creation time — matching the order `PageComments` renders them.
   *
   *  Resolved threads are excluded, except the active one: it holds its slot so
   *  that resolving the thread you're navigating on steps to the *next* thread
   *  rather than dropping out from under you (which `resolveNavTarget` would
   *  read as an unknown id and answer with idle entry — the first thread on
   *  `next`, the last on `prev`). It leaves the list as soon as `activeId`
   *  moves off it. */
  get navigable(): string[] {
    const inline = sortByOrder(
      this.inlineThreads.filter((t) => holdsSlot(t, this.activeId)),
      this.order,
    ).map((t) => t.id);
    const page = this.pageThreads
      .filter((t) => holdsSlot(t, this.activeId))
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
    const target = resolveNavTarget(this.navigable, this.activeId, direction);
    if (target == null) return null;
    this.activeId = target;
    this.navSeq++;
    // Recomputed after the move, not reused from the pre-move read: `holdsSlot`
    // keeps a resolved thread in the list only while it's active, so stepping
    // off the thread just resolved drops it from the list the announcement must
    // describe. `navigable` is a plain getter over `$state`, so this genuinely
    // re-reads.
    const list = this.navigable;
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
    this.pendingLeft = null;
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
    // Abort any in-flight load so a list() resolving after clear() is discarded
    // at its abort check instead of repopulating the just-cleared list with the
    // previous document's comments — e.g. when navigating to a page that shows
    // no comments and never re-triggers a load.
    this.abortController?.abort();
    this.abortController = null;
    this.items = [];
    this.unseenWrites.clear();
    this.tombstones.clear();
    this.loading = false;
    this.activeId = null;
    this.linkedId = null;
    this.resolvedExpanded = false;
    this.activeTop = null;
    this.activeLeft = null;
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
