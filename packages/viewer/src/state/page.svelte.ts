import type { PageResponse } from "../types";
import type { ApiClient } from "../api/client";
import { NotFoundError } from "../api/client";
import type { SectionRefResolver } from "$lib/sectionRefs";
import { resolveBreadcrumbs } from "$lib/sectionRefs";

export class Page {
  data = $state.raw<PageResponse | null>(null);
  loading = $state(false);
  error = $state<string | null>(null);
  notFound = $state(false);

  readonly sectionRefResolver?: SectionRefResolver;

  private apiClient: ApiClient;
  private embedded: boolean;
  private abortController: AbortController | null = null;

  constructor(
    apiClient: ApiClient,
    options?: { embedded?: boolean; sectionRefResolver?: SectionRefResolver },
  ) {
    this.apiClient = apiClient;
    this.embedded = options?.embedded ?? false;
    this.sectionRefResolver = options?.sectionRefResolver;
  }

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
      let data = await this.apiClient.fetchPage(path, {
        bypassCache: options?.bypassCache,
        signal,
      });
      if (signal.aborted) return;
      if (this.sectionRefResolver) {
        try {
          const breadcrumbs = await resolveBreadcrumbs(
            data.breadcrumbs,
            data.sectionAncestry,
            this.sectionRefResolver,
          );
          if (signal.aborted) return;
          data = { ...data, breadcrumbs };
        } catch (e) {
          if (signal.aborted) return; // superseded load: don't commit
          if (import.meta.env.DEV) {
            console.warn(
              "[rw] breadcrumb section-ref resolution failed; using unresolved breadcrumbs:",
              e,
            );
          }
          // keep data.breadcrumbs as fetched
        }
      }
      this.settle(data);
      // The `PageResponse` type says `meta.title` is always present, but an
      // older `rw serve` build (rolling deploy, or a proxy/service-worker
      // replaying a cached response) can still send `title: null`. A missing
      // or empty title leaves the tab alone instead of showing "null - RW" or
      // " - RW".
      if (data.meta.title && !this.embedded) {
        document.title = `${data.meta.title} - RW`;
      }
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
