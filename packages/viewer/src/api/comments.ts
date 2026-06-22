import type { Comment, CreateCommentRequest, UpdateCommentRequest } from "../types/comments";

/**
 * Host-supplied comment transport for `mountRw({ comments })`.
 *
 * **Security — the host owns sanitization.** Each returned `Comment.bodyHtml` is
 * injected into the doc viewer as trusted HTML (`{@html}`) with no client-side
 * sanitization. A host implementing this client MUST return `bodyHtml` already
 * sanitized to a safe, restricted subset, or omit it to fall back to the
 * plain-text `body`. Render bodies with `renderCommentBody` from `@rwdocs/core`
 * to match the subset the default `rw serve` backend produces. Returning
 * unsanitized or upstream-proxied HTML produces stored XSS in the host page's
 * origin.
 */
export interface CommentApiClient {
  list(documentId: string, options?: { signal?: AbortSignal }): Promise<Comment[]>;
  create(input: CreateCommentRequest): Promise<Comment>;
  update(id: string, input: UpdateCommentRequest): Promise<Comment>;
  delete(id: string): Promise<Comment>;
  /** Optional host-driven live refresh. Called with the current document id and
   *  a callback to invoke when its comments change; returns an unsubscribe
   *  handle. When absent, the viewer falls back to the live-reload WebSocket. */
  subscribe?(documentId: string, onChange: () => void): () => void;
}

export function createCommentApiClient(
  apiBase: string = "/_api",
  fetchFn?: typeof fetch,
): CommentApiClient {
  const doFetch = fetchFn ?? fetch;
  const base = apiBase.replace(/\/+$/, "");

  return {
    async list(documentId: string, options?: { signal?: AbortSignal }): Promise<Comment[]> {
      const params = new URLSearchParams({ documentId });
      const response = await doFetch(`${base}/comments?${params}`, {
        signal: options?.signal,
      });
      if (!response.ok) {
        throw new Error(`Failed to fetch comments: ${response.status}`);
      }
      return response.json();
    },

    async create(input: CreateCommentRequest): Promise<Comment> {
      const response = await doFetch(`${base}/comments`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(input),
      });
      if (!response.ok) {
        throw new Error(`Failed to create comment: ${response.status}`);
      }
      return response.json();
    },

    async update(id: string, input: UpdateCommentRequest): Promise<Comment> {
      const response = await doFetch(`${base}/comments/${id}`, {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(input),
      });
      if (!response.ok) {
        throw new Error(`Failed to update comment: ${response.status}`);
      }
      return response.json();
    },

    async delete(id: string): Promise<Comment> {
      const response = await doFetch(`${base}/comments/${id}`, {
        method: "DELETE",
      });
      if (!response.ok) {
        throw new Error(`Failed to delete comment: ${response.status}`);
      }
      return response.json();
    },
  };
}
