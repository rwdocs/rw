import type { Breadcrumb, PageResponse } from "../types";
import type { ApiClient } from "../api/client";
import { NotFoundError } from "../api/client";
import { encodePath } from "$lib/encodePath";
import { SectionLinks } from "$lib/sectionRefs";

export class Page {
  data = $state.raw<PageResponse | null>(null);
  loading = $state(false);
  error = $state<string | null>(null);
  notFound = $state(false);

  readonly links: SectionLinks;

  private apiClient: ApiClient;
  private abortController: AbortController | null = null;

  constructor(apiClient: ApiClient, links = new SectionLinks()) {
    this.apiClient = apiClient;
    this.links = links;
  }

  /** The host's URL for a crumb, when the host maps its section. */
  crumbHref = (crumb: Breadcrumb): string | undefined =>
    this.links.href(crumb.sectionRef, encodePath(crumb.subpath), this.data?.sectionAncestry ?? {});

  load = async (path: string, options?: { bypassCache?: boolean; silent?: boolean }) => {
    if (this.abortController) {
      this.abortController.abort();
    }
    this.abortController = new AbortController();
    const signal = this.abortController.signal;

    if (!options?.silent) {
      this.loading = true;
      this.error = null;
      this.notFound = false;
    }

    try {
      const data = await this.apiClient.fetchPage(path, {
        bypassCache: options?.bypassCache,
        signal,
      });
      if (signal.aborted) return;
      await this.links.ensure(data.sectionAncestry);
      if (signal.aborted) return;
      this.settle(data);
    } catch (e) {
      // Judge supersession by our signal, not the error: a superseded fetch can
      // reject with a plain Error, and a host fetchFn can raise its own AbortError.
      if (signal.aborted) return;
      if (options?.silent) {
        // Silent (background/live-reload) refresh failed: keep the
        // last-known-good page on screen instead of blanking it — data,
        // error, and notFound are intentionally left untouched.
        if (import.meta.env.DEV) {
          console.warn("[rw] silent page refresh failed; keeping current page:", e);
        }
        // A silent refresh can supersede an in-flight *non-silent* load that
        // set loading=true; clear that spinner so it doesn't stick.
        this.loading = false;
        return;
      }
      if (e instanceof NotFoundError) {
        this.settle(null, null, true);
      } else {
        this.settle(null, e instanceof Error ? e.message : "Unknown error");
      }
    } finally {
      if (this.abortController?.signal === signal) {
        this.abortController = null;
      }
    }
  };

  clear = () => {
    this.settle(null);
  };

  private settle(data: PageResponse | null, error: string | null = null, notFound = false) {
    this.data = data;
    this.loading = false;
    this.error = error;
    this.notFound = notFound;
  }
}
