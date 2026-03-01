import Router from "express-promise-router";
import type { HttpAuthService, LoggerService } from "@backstage/backend-plugin-api";
import { createSite, type RwSite, type SiteConfig } from "@rw/core";

export interface S3Options {
  bucket: string;
  entity: string;
  region?: string;
  endpoint?: string;
  bucketRootPath?: string;
}

export interface RouterOptions {
  logger: LoggerService;
  httpAuth: HttpAuthService;
  projectDir?: string;
  s3?: S3Options;
  linkPrefix?: string;
}

export async function createRouter(options: RouterOptions) {
  const { logger, projectDir, s3, linkPrefix } = options;
  const router = Router();

  const config: SiteConfig = { projectDir, s3, linkPrefix };
  logger.info(
    s3
      ? `Creating RW site from S3 (${s3.bucket}/${s3.entity})`
      : `Creating RW site from ${projectDir}`,
  );
  const site: RwSite = createSite(config);

  router.get("/health", (_req, res) => {
    res.json({ status: "ok" });
  });

  router.get("/config", (_req, res) => {
    res.json({ liveReloadEnabled: false });
  });

  router.get("/navigation", (req, res) => {
    const scopeParam = req.query.scope;
    const scope = typeof scopeParam === "string" ? scopeParam : undefined;
    const nav = site.getNavigation(scope ?? null);
    res.json(nav);
  });

  router.get("/pages/", async (_req, res) => {
    try {
      const page = await site.renderPage("");
      res.json(page);
    } catch (err) {
      handlePageError(err, "/", res, logger);
    }
  });

  router.get("/pages/:path(*)", async (req, res) => {
    const pagePath = req.params.path || "";
    if (pagePath.split("/").includes("..")) {
      res.status(400).json({ error: "Invalid path" });
      return;
    }
    try {
      const page = await site.renderPage(pagePath);
      res.json(page);
    } catch (err) {
      handlePageError(err, `/${pagePath}`, res, logger);
    }
  });

  return router;
}

function handlePageError(
  err: unknown,
  path: string,
  res: import("express").Response,
  logger: LoggerService,
) {
  const message = err instanceof Error ? err.message : String(err);
  if (message.includes("Content not found")) {
    res.status(404).json({ error: "Page not found", path });
  } else {
    logger.error(`Failed to render page ${path}: ${message}`);
    res.status(500).json({ error: "Internal server error" });
  }
}
