import { createContext } from "svelte";
import type { Router } from "../state/router.svelte";
import type { Page } from "../state/page.svelte";
import type { Navigation } from "../state/navigation.svelte";
import type { LiveReload } from "../state/liveReload.svelte";
import type { Ui } from "../state/ui.svelte";
import type { Comments } from "../state/comments.svelte";
import type { NotifyFn } from "../types/notify";

export interface RwContext {
  router: Router;
  page: Page;
  navigation: Navigation;
  liveReload: LiveReload;
  ui: Ui;
  comments: Comments;
  notify: NotifyFn;
}

// `getRwContext()` throws if no ancestor set the context; `App.svelte` always
// calls `setRwContext()` at the root, so every consumer is covered.
export const [getRwContext, setRwContext] = createContext<RwContext>();
