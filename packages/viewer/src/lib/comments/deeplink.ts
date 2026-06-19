import type { Comment } from "../../types/comments";

const PREFIX = "comment-";

/** Build the URL hash (without leading '#') for a comment id. */
export function buildCommentHash(id: string): string {
  return `${PREFIX}${id}`;
}

/** Extract the comment id from a hash (leading '#' optional), or null if the
 *  hash is not a non-empty `comment-`-prefixed hash. */
export function parseCommentHash(hash: string): string | null {
  const h = hash.startsWith("#") ? hash.slice(1) : hash;
  if (!h.startsWith(PREFIX)) return null;
  const id = h.slice(PREFIX.length);
  return id.length > 0 ? id : null;
}

/** True when `hash` is a comment hash whose id appears in `ids`. */
export function isCommentHash(hash: string, ids: Iterable<string>): boolean {
  const id = parseCommentHash(hash);
  if (id === null) return false;
  for (const known of ids) {
    if (known === id) return true;
  }
  return false;
}

export type CommentTargetKind = "inline" | "page" | "resolved" | "missing";

/** Classify how a deep-link target should be revealed.
 *  - "missing": no such comment loaded (deleted, wrong page, or still loading)
 *  - "resolved": reveal via the "Show resolved" disclosure
 *  - "inline": open inline comment currently anchored in the article
 *  - "page": open page-level or open orphaned-inline comment in the timeline */
export function classifyCommentTarget(
  comment: Comment | undefined,
  isAnchoredInline: boolean,
): CommentTargetKind {
  if (!comment) return "missing";
  if (comment.status === "resolved") return "resolved";
  if (isAnchoredInline) return "inline";
  return "page";
}
