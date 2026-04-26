import type { AnchorStrategy } from "$lib/anchoring";
import type { CommentApiClient } from "../api/comments";
import type { Comment, CreateCommentRequest, Selector } from "../types/comments";

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
  error = $state<string | null>(null);
  activeId = $state<string | null>(null);
  /** Vertical offset of the active highlight relative to the content area. */
  activeTop = $state<number | null>(null);
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

  private apiClient: CommentApiClient;
  private abortController: AbortController | null = null;
  private documentId: string | null = null;

  constructor(apiClient: CommentApiClient) {
    this.apiClient = apiClient;
  }

  load = async (documentId: string) => {
    if (!this.enabled) return;
    if (this.abortController) {
      this.abortController.abort();
    }
    this.abortController = new AbortController();
    const signal = this.abortController.signal;

    if (documentId !== this.documentId) {
      this.activeId = null;
      this.clearPending();
      this.documentId = documentId;
    }
    this.loading = true;
    this.error = null;
    try {
      const items = await this.apiClient.list(documentId, { signal });
      if (signal.aborted) return;
      this.items = items;
    } catch (e) {
      if (e instanceof DOMException && e.name === "AbortError") return;
      this.error = e instanceof Error ? e.message : "Failed to load comments";
      this.items = [];
    } finally {
      if (this.abortController?.signal === signal) {
        this.abortController = null;
      }
      if (!signal.aborted) {
        this.loading = false;
      }
    }
  };

  create = async (input: CreateCommentRequest) => {
    const comment = await this.apiClient.create(input);
    this.items = [...this.items, comment];
    return comment;
  };

  resolve = async (id: string) => {
    const updated = await this.apiClient.update(id, { status: "resolved" });
    this.items = this.items.map((c) => (c.id === id ? updated : c));
  };

  reopen = async (id: string) => {
    const updated = await this.apiClient.update(id, { status: "open" });
    this.items = this.items.map((c) => (c.id === id ? updated : c));
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

  replies(parentId: string): Comment[] {
    return this.items.filter((c) => c.parentId === parentId);
  }

  clearPending = () => {
    this.pending = null;
    this.pendingTop = null;
  };

  clear = () => {
    this.items = [];
    this.loading = false;
    this.error = null;
    this.activeId = null;
    this.activeTop = null;
    this.order = [];
    this.anchorStrategies = new Map();
    this.orphanIds = new Set();
    this.documentId = null;
    this.clearPending();
  };
}
