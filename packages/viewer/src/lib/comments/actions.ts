import type { Comments } from "../../state/comments.svelte";
import type { Comment, CreateCommentRequest } from "../../types/comments";
import type { NotifyFn } from "../../types/notify";
import { SAVE_FAILED_MESSAGE } from "./messages";

/** The comment-mutation callbacks shared by PageComments and CommentPanel.
 *  The four status/delete actions wrap the store call in a notify-and-swallow
 *  (the user-facing toast is the only failure surface); `create` and `reply`
 *  notify and **rethrow** so the composer keeps the draft. */
export interface CommentActions {
  create: (input: CreateCommentRequest) => Promise<Comment>;
  resolve: (id: string) => Promise<void>;
  reopen: (id: string) => Promise<void>;
  /** Soft-delete (maps to `comments.delete`). Named `remove` to avoid the
   *  reserved-word feel at call sites. */
  remove: (id: string) => Promise<void>;
  restore: (id: string) => Promise<void>;
  reply: (parentId: string, body: string) => Promise<void>;
}

/** Build the shared mutation handlers over a `comments` store and a `notify`
 *  sink. Both come from `getRwContext()` and are stable for the component's
 *  life, so this is called once per component (no reactive state of its own —
 *  hence a plain `.ts` factory, matching the other `$lib/comments/*` helpers). */
export function createCommentActions(comments: Comments, notify: NotifyFn): CommentActions {
  const swallow = async (run: () => Promise<unknown>, message: string) => {
    try {
      await run();
    } catch (e) {
      notify({ intent: "error", message: e instanceof Error ? e.message : message });
    }
  };

  const create = async (input: CreateCommentRequest) => {
    try {
      return await comments.create(input);
    } catch (e) {
      notify({ intent: "error", message: SAVE_FAILED_MESSAGE });
      throw e;
    }
  };

  return {
    create,
    resolve: (id) => swallow(() => comments.resolve(id), "Failed to resolve comment"),
    reopen: (id) => swallow(() => comments.reopen(id), "Failed to reopen comment"),
    remove: (id) => swallow(() => comments.delete(id), "Failed to delete comment"),
    restore: (id) => swallow(() => comments.restore(id), "Failed to restore comment"),
    reply: async (parentId, body) => {
      const thread = comments.threads.find((t) => t.id === parentId);
      if (!thread) return;
      await create({ documentId: thread.documentId, parentId, body, selectors: [] });
    },
  };
}
