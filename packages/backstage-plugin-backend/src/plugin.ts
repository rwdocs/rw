import { coreServices, createBackendPlugin } from "@backstage/backend-plugin-api";
import { createRouter, type S3Options } from "./router";

export const rwPlugin = createBackendPlugin({
  pluginId: "rw",
  register(env) {
    env.registerInit({
      deps: {
        httpRouter: coreServices.httpRouter,
        httpAuth: coreServices.httpAuth,
        logger: coreServices.logger,
        config: coreServices.rootConfig,
      },
      async init({ httpRouter, httpAuth, logger, config }) {
        const projectDir = config.getOptionalString("rw.projectDir");

        let s3: S3Options | undefined;
        const s3Config = config.getOptionalConfig("rw.s3");
        if (s3Config) {
          s3 = {
            bucket: s3Config.getString("bucket"),
            entity: s3Config.getString("entity"),
            region: s3Config.getOptionalString("region"),
            endpoint: s3Config.getOptionalString("endpoint"),
            bucketRootPath: s3Config.getOptionalString("bucketRootPath"),
          };
        }

        if (!projectDir && !s3) {
          throw new Error("Either rw.projectDir or rw.s3 must be configured");
        }

        const linkPrefix = config.getOptionalString("rw.linkPrefix");
        if (linkPrefix) {
          logger.info(`Using link prefix: ${linkPrefix}`);
        }
        const router = await createRouter({ logger, httpAuth, projectDir, s3, linkPrefix });
        httpRouter.use(router);
        httpRouter.addAuthPolicy({
          path: "/health",
          allow: "unauthenticated",
        });
      },
    });
  },
});
