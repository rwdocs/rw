import { test } from "node:test";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const { createSite } = require("../index.js");

// A republish that changes only an `!include`d file leaves the page's mtime
// alone; only the bundle's hash in the manifest moves.
test("S3 renderPage shows a republished bundle whose mtime did not move", async (t) => {
  const objects = new Map();
  let manifestVersion = 0;
  const publish = (content, sha256) => {
    objects.set("pages/guide.json", { body: JSON.stringify({ content }) });
    const documents = [{ path: "guide", title: "Guide", has_content: true, is_dir: false }];
    objects.set("manifest.json", {
      body: JSON.stringify({
        version: 1,
        documents,
        mtimes: { guide: 1713000000 },
        bundles: { guide: sha256 },
      }),
      etag: `"manifest-${++manifestVersion}"`,
    });
  };
  const server = createServer(async (req, res) => {
    const key = new URL(req.url, "http://localhost").pathname.replace("/bucket/docs/", "");
    const chunks = [];
    for await (const chunk of req) chunks.push(chunk);
    if (req.method === "PUT") {
      objects.set(key, {
        body: Buffer.concat(chunks),
        cacheEtag: req.headers["x-amz-meta-cache-etag"],
      });
      res.end();
      return;
    }
    const object = objects.get(key);
    if (!object) {
      res.writeHead(404);
      res.end();
      return;
    }
    res.writeHead(200, {
      ...(object.etag && { ETag: object.etag }),
      ...(object.cacheEtag && { "x-amz-meta-cache-etag": object.cacheEtag }),
    });
    res.end(object.body);
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  t.after(() => {
    server.closeAllConnections();
    server.close();
  });
  publish("# Guide\n\n```plantuml\nA -> B\n```\n", "a".repeat(64));
  const site = createSite({
    s3: {
      bucket: "bucket",
      entity: "docs",
      endpoint: `http://127.0.0.1:${server.address().port}`,
      accessKeyId: "dummy-key",
      secretAccessKey: "dummy-secret",
    },
  });

  assert.match((await site.renderPage("guide")).content, /A -&gt; B/);
  publish("# Guide\n\n```plantuml\nC -> D\n```\n", "b".repeat(64));
  assert.equal(await site.reload(false), true);

  const republished = await site.renderPage("guide");
  assert.match(republished.content, /C -&gt; D/);
});
