import type { Comment, CreateCommentRequest, UpdateCommentRequest } from "../types/comments";

export interface CommentApiClient {
  list(documentId: string, options?: { signal?: AbortSignal }): Promise<Comment[]>;
  create(input: CreateCommentRequest): Promise<Comment>;
  update(id: string, input: UpdateCommentRequest): Promise<Comment>;
}

export function createCommentApiClient(
  apiBase: string = "/api",
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
  };
}
