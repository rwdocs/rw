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
  selectors: Selector[];
  status: CommentStatus;
  createdAt: string;
  updatedAt: string;
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
