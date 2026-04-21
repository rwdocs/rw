import { getContext, setContext } from "svelte";
import type { ApiClient } from "../api/client";
import type { Router } from "../state/router.svelte";
import type { Page } from "../state/page.svelte";
import type { Navigation } from "../state/navigation.svelte";
import type { LiveReload } from "../state/liveReload.svelte";
import type { Ui } from "../state/ui.svelte";
import type { Comments } from "../state/comments.svelte";

export interface RwContext {
  apiClient: ApiClient;
  router: Router;
  page: Page;
  navigation: Navigation;
  liveReload: LiveReload;
  ui: Ui;
  comments: Comments;
  resolveSectionRefs?: (refs: string[]) => Promise<Record<string, string>>;
}

const RW_KEY = Symbol("rw");

export function setRwContext(ctx: RwContext) {
  setContext(RW_KEY, ctx);
}

export function getRwContext(): RwContext {
  return getContext<RwContext>(RW_KEY);
}
