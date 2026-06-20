import { createCommentApiClient, type CommentApiClient } from "../../api/comments";

/**
 * Pick the comment client the viewer should use.
 *
 * When the host injects a `CommentApiClient`, comments are implicitly enabled
 * (no `/config` round-trip) and that client owns transport, URLs, auth, and
 * live refresh. Otherwise the viewer builds today's default HTTP client against
 * the docs API base, and enablement is deferred to the `/config` `commentsEnabled`
 * flag (so `enabled` is reported as `false` here).
 */
export function selectCommentClient(
  injected: CommentApiClient | undefined,
  apiBaseUrl: string,
  fetchFn?: typeof fetch,
): { client: CommentApiClient; enabled: boolean } {
  if (injected) {
    return { client: injected, enabled: true };
  }
  return { client: createCommentApiClient(apiBaseUrl, fetchFn), enabled: false };
}
