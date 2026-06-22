import type { Comments } from "../../state/comments.svelte";
import type { NotifyFn } from "../../types/notify";
import { SAVE_FAILED_MESSAGE } from "./messages";

/** The five comment-mutation callbacks shared by PageComments and
 *  CommentSidebar. The four status/delete actions wrap the store call in a
 *  notify-and-swallow (the user-facing toast is the only failure surface);
 *  `reply` notifies and **rethrows** so the composer keeps the draft. */
export interface CommentActions {
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

  return {
    resolve: (id) => swallow(() => comments.resolve(id), "Failed to resolve comment"),
    reopen: (id) => swallow(() => comments.reopen(id), "Failed to reopen comment"),
    remove: (id) => swallow(() => comments.delete(id), "Failed to delete comment"),
    restore: (id) => swallow(() => comments.restore(id), "Failed to restore comment"),
    reply: async (parentId, body) => {
      const thread = comments.threads.find((t) => t.id === parentId);
      if (!thread) return;
      try {
        await comments.create({
          documentId: thread.documentId,
          parentId,
          body,
          selectors: [],
        });
      } catch (e) {
        notify({ intent: "error", message: SAVE_FAILED_MESSAGE });
        throw e;
      }
    },
  };
}
