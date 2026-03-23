import type { PageResponse } from "../types";
import type { ApiClient } from "../api/client";
import { NotFoundError } from "../api/client";
import type { SectionRefResolver } from "../lib/sectionRefs";
import { resolveBreadcrumbs } from "../lib/sectionRefs";

export class Page {
  data = $state.raw<PageResponse | null>(null);
  loading = $state(false);
  error = $state<string | null>(null);
  notFound = $state(false);

  private apiClient: ApiClient;
  private embedded: boolean;
  private abortController: AbortController | null = null;
  private sectionRefResolver?: SectionRefResolver;

  constructor(apiClient: ApiClient, options?: { embedded?: boolean }) {
    this.apiClient = apiClient;
    this.embedded = options?.embedded ?? false;
  }

  /** Configure section ref resolution for breadcrumb path rewriting. */
  setSectionRefResolver(resolver: SectionRefResolver) {
    this.sectionRefResolver = resolver;
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
        data = {
          ...data,
          breadcrumbs: await resolveBreadcrumbs(data.breadcrumbs, this.sectionRefResolver),
        };
        if (signal.aborted) return;
      }
      this.data = data;
      this.loading = false;
      this.error = null;
      this.notFound = false;
      if (data.meta.title && !this.embedded) {
        document.title = `${data.meta.title} - RW`;
      }
    } catch (e) {
      if (e instanceof DOMException && e.name === "AbortError") {
        return;
      }
      if (e instanceof NotFoundError) {
        this.data = null;
        this.loading = false;
        this.error = null;
        this.notFound = true;
      } else {
        const message = e instanceof Error ? e.message : "Unknown error";
        this.data = null;
        this.loading = false;
        this.error = message;
        this.notFound = false;
      }
    } finally {
      if (this.abortController?.signal === signal) {
        this.abortController = null;
      }
    }
  };

  clear = () => {
    this.data = null;
    this.loading = false;
    this.error = null;
    this.notFound = false;
  };
}
