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
  /** Server-rendered HTML of `body` (restricted markdown). Safe to inject.
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
}

export interface CreateCommentRequest {
  documentId: string;
  parentId?: string;
  body: string;
  selectors: Selector[];
}

export interface UpdateCommentRequest {
  body?: string;
  status?: "open" | "resolved";
  selectors?: Selector[];
}
