/** A selector anchoring a comment to text. */
export type Selector =
  | { type: "TextQuoteSelector"; exact: string; prefix: string; suffix: string }
  | { type: "TextPositionSelector"; start: number; end: number }
  | { type: "CSSSelector"; value: string };

export type CommentStatus = "open" | "resolved";

export interface Author {
  id: string;
  name: string;
  avatarUrl?: string;
}

export interface Comment {
  id: string;
  documentId: string;
  parentId?: string;
  author: Author;
  body: string;
  /** Server-rendered HTML of `body`, injected into the DOM as trusted HTML
   *  (`{@html}`) with **no** client-side sanitization. Whoever produces this
   *  field owns sanitizing it to a safe, restricted subset. The default
   *  `rw serve` backend does so via `renderCommentBody` (`@rwdocs/core`); a host
   *  that supplies its own `CommentApiClient` must return sanitized HTML here.
   *  Returning unsanitized or upstream-proxied HTML is a stored-XSS vector in
   *  the host page's origin.
   *  Optional: a backend that doesn't render comments server-side may omit it,
   *  in which case the viewer falls back to the plain-text `body`. */
  bodyHtml?: string;
  selectors: Selector[];
  status: CommentStatus;
  createdAt: string;
  updatedAt: string;
  /** Soft-delete timestamp. Set when the comment was deleted; omitted on live
   *  rows. The canonical "is deleted" signal is `deletedAt != null`. */
  deletedAt?: string | null;
  canDelete: boolean;
  canRestore: boolean;
  /** Whether the viewer should offer the Resolve/Reopen affordance for this
   *  comment. Server-driven (like `canDelete`/`canRestore`): the host decides,
   *  the viewer renders. The default `rw serve` backend sets this true for
   *  top-level comments and false for replies. */
  canResolve: boolean;
}

export interface CreateCommentRequest {
  documentId: string;
  parentId?: string;
  body: string;
  selectors: Selector[];
}

export interface UpdateCommentRequest {
  body?: string;
  status?: CommentStatus;
  selectors?: Selector[];
}
