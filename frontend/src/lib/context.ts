import { getContext, setContext } from "svelte";
import type { ApiClient } from "../api/client";
import type { RouterStore } from "../stores/router";
import type { PageStore } from "../stores/page";
import type { NavigationStore } from "../stores/navigation";
import type { LiveReloadStore } from "../stores/liveReload";
import type { UiStore } from "../stores/ui";

export interface RwContext {
  apiClient: ApiClient;
  router: RouterStore;
  page: PageStore;
  navigation: NavigationStore;
  liveReload: LiveReloadStore;
  ui: UiStore;
}

const RW_KEY = Symbol("rw");

export function setRwContext(ctx: RwContext) {
  setContext(RW_KEY, ctx);
}

export function getRwContext(): RwContext {
  return getContext<RwContext>(RW_KEY);
}
