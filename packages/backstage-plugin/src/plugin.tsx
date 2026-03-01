import {
  createFrontendPlugin,
  PageBlueprint,
  createRouteRef,
  ApiBlueprint,
} from "@backstage/frontend-plugin-api";
import { createApiFactory, discoveryApiRef, fetchApiRef } from "@backstage/core-plugin-api";
import { EntityContentBlueprint } from "@backstage/plugin-catalog-react/alpha";
import { rwApiRef, RwClient } from "./api/RwClient";

const rootRouteRef = createRouteRef();

const rwApi = ApiBlueprint.make({
  params: (defineParams) =>
    defineParams(
      createApiFactory({
        api: rwApiRef,
        deps: { discoveryApi: discoveryApiRef, fetchApi: fetchApiRef },
        factory: ({ discoveryApi, fetchApi }) => new RwClient({ discoveryApi, fetchApi }),
      }),
    ),
});

const rwPage = PageBlueprint.make({
  params: {
    path: "/docs",
    routeRef: rootRouteRef,
    loader: () => import("./components/RwDocsViewer").then((m) => <m.RwDocsViewer />),
  },
});

const rwEntityContent = EntityContentBlueprint.make({
  params: {
    path: "docs",
    title: "Documentation",
    loader: () => import("./components/RwDocsViewer").then((m) => <m.RwDocsViewer />),
  },
});

export const rwPlugin = createFrontendPlugin({
  pluginId: "rw",
  extensions: [rwApi, rwPage, rwEntityContent],
  routes: { root: rootRouteRef },
});

export default rwPlugin;
