// `.js` is deliberate — this type reaches the published `.d.ts`. See `src/embed.ts`.
import type { Comment, CreateCommentRequest, UpdateCommentRequest } from "../types/comments.js";
import { readJson } from "./json";

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
  /** Oldest first — the viewer renders replies in the order they arrive, so a
   *  client answering newest-first shows every thread inside out. */
  list(documentId: string, options?: { signal?: AbortSignal }): Promise<Comment[]>;
  create(input: CreateCommentRequest): Promise<Comment>;
  update(id: string, input: UpdateCommentRequest): Promise<Comment>;
  delete(id: string): Promise<Comment>;
  /** Optional host-driven live refresh. Called with the current document id and
   *  a callback to invoke when its comments change; returns an unsubscribe
   *  handle. When absent, comments do not refresh live. */
  subscribe?(documentId: string, onChange: () => void): () => void;
}

function jsonRequest(method: "POST" | "PATCH", body: unknown): RequestInit {
  return { method, headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) };
}

export function createCommentApiClient(
  apiBase: string,
  fetchFn: typeof fetch = fetch,
): CommentApiClient {
  const base = apiBase.replace(/\/+$/, "");

  return {
    async list(documentId: string, options?: { signal?: AbortSignal }): Promise<Comment[]> {
      const params = new URLSearchParams({ documentId });
      const response = await fetchFn(`${base}/comments?${params}`, { signal: options?.signal });
      return readJson<Comment[]>(response, "Failed to fetch comments");
    },

    async create(input: CreateCommentRequest): Promise<Comment> {
      const response = await fetchFn(`${base}/comments`, jsonRequest("POST", input));
      return readJson<Comment>(response, "Failed to create comment");
    },

    async update(id: string, input: UpdateCommentRequest): Promise<Comment> {
      const response = await fetchFn(`${base}/comments/${id}`, jsonRequest("PATCH", input));
      return readJson<Comment>(response, "Failed to update comment");
    },

    async delete(id: string): Promise<Comment> {
      const response = await fetchFn(`${base}/comments/${id}`, { method: "DELETE" });
      return readJson<Comment>(response, "Failed to delete comment");
    },
  };
}
