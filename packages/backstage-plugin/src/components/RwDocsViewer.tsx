import { useRef, useEffect, useState } from "react";
import { useApi } from "@backstage/core-plugin-api";
import { ErrorPanel } from "@backstage/core-components";
import { useLocation, useNavigate, useParams } from "react-router-dom";
import { rwApiRef } from "../api/RwClient";
import { mountRw } from "@rw/viewer";
import type { RwInstance } from "@rw/viewer";
import "@rw/viewer/embed.css";

export function RwDocsViewer() {
  const ref = useRef<HTMLDivElement>(null);
  const rwApi = useApi(rwApiRef);
  const [error, setError] = useState<Error | null>(null);

  const location = useLocation();
  const navigate = useNavigate();
  const navigateRef = useRef(navigate);
  navigateRef.current = navigate;
  const { "*": subPath = "" } = useParams();

  // Derive the plugin's base path by stripping the sub-path (and its leading slash) from the URL.
  // e.g. URL="/rw-docs/getting-started", subPath="getting-started" → base="/rw-docs"
  const basePath = subPath ? location.pathname.slice(0, -(subPath.length + 1)) : location.pathname;
  const basePathRef = useRef(basePath);
  basePathRef.current = basePath;

  const instanceRef = useRef<RwInstance | null>(null);
  const prevSubPathRef = useRef(subPath);
  const rwNavigatingRef = useRef(false);

  useEffect(() => {
    let cancelled = false;

    rwApi
      .getBaseUrl()
      .then((baseUrl) => {
        if (cancelled || !ref.current) return;

        const base = basePathRef.current;
        const initialPath = subPath ? `/${subPath}` : "/";

        instanceRef.current = mountRw(ref.current, {
          apiBaseUrl: baseUrl,
          initialPath,
          basePath: base,
          fetchFn: rwApi.getFetch(),
          onNavigate: (rwPath: string) => {
            const browserPath = rwPath === "/" ? base : `${base}${rwPath}`;
            if (window.location.pathname !== browserPath) {
              rwNavigatingRef.current = true;
              navigateRef.current(browserPath, { replace: false });
            }
          },
        });
      })
      .catch((err) => {
        if (!cancelled) setError(err);
      });

    return () => {
      cancelled = true;
      instanceRef.current?.destroy();
      instanceRef.current = null;
    };
    // Intentionally mount once — back/forward sync is handled by the effect below
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rwApi]);

  // Sync external navigation (browser back/forward) to the RW app
  useEffect(() => {
    if (subPath === prevSubPathRef.current) return;
    prevSubPathRef.current = subPath;

    if (rwNavigatingRef.current) {
      rwNavigatingRef.current = false;
      return;
    }

    // External navigation — tell RW to navigate
    const rwPath = subPath ? `/${subPath}` : "/";
    instanceRef.current?.navigateTo(rwPath);
  }, [subPath]);

  if (error) {
    return <ErrorPanel error={error} />;
  }

  return <div ref={ref} className="rw-root" style={{ height: "100%" }} />;
}
