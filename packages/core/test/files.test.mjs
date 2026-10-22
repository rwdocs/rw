import { test } from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import fs from "node:fs";
import { createServer } from "node:http";
import os from "node:os";
import path from "node:path";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const { createSite } = require("../index.js");

const PNG = Buffer.from("\x89PNG\r\n\x1a\n", "latin1");
const SHA256 = createHash("sha256").update(PNG).digest("hex");

function projectSite(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "rw-core-files-"));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const projectDir = path.join(root, "project");
  fs.mkdirSync(path.join(projectDir, "docs", "img"), { recursive: true });
  fs.writeFileSync(path.join(projectDir, "docs", "index.md"), "# Home\n\n![Flow](img/flow.png)\n");
  fs.writeFileSync(path.join(projectDir, "docs", "img", "flow.png"), PNG);
  fs.writeFileSync(path.join(projectDir, "docs", "flow.puml"), "@startuml\nA -> B\n@enduml\n");
  fs.writeFileSync(path.join(projectDir, ".env"), "SECRET=1\n");
  fs.writeFileSync(path.join(root, "outside.png"), PNG);
  return { site: createSite({ projectDir }), root, projectDir };
}

test("getFile serves an allowlisted project file with its type and a SHA-256 etag", async (t) => {
  const { site } = projectSite(t);

  const file = await site.getFile("docs/img/flow.png");

  assert.ok(Buffer.isBuffer(file.content));
  assert.deepEqual(file.content, PNG);
  assert.equal(file.contentType, "image/png");
  assert.equal(file.etag, `"${SHA256}"`);
});

test("getFile gives the headers rw serve sends", async (t) => {
  const { site, projectDir } = projectSite(t);
  fs.writeFileSync(
    path.join(projectDir, "docs", "index.md"),
    "# Home\n\n[Plan](plan%20v2.xlsx) [Spec](spec.pdf)\n",
  );
  fs.writeFileSync(path.join(projectDir, "docs", "plan v2.xlsx"), "xlsx");
  fs.writeFileSync(path.join(projectDir, "docs", "spec.pdf"), "pdf");
  const etag = (content) => `"${createHash("sha256").update(content).digest("hex")}"`;
  const served = {
    "cache-control": "no-cache",
    "content-security-policy": "sandbox",
    "x-content-type-options": "nosniff",
  };

  assert.deepEqual((await site.getFile("docs/plan v2.xlsx")).headers, {
    ...served,
    "content-type": "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    "content-disposition": `attachment; filename="plan v2.xlsx"; filename*=UTF-8''plan%20v2.xlsx`,
    etag: etag("xlsx"),
  });
  assert.deepEqual((await site.getFile("docs/spec.pdf")).headers, {
    ...served,
    "content-type": "application/pdf",
    etag: etag("pdf"),
  });
  assert.deepEqual((await site.getFile("docs/img/flow.png")).headers, {
    ...served,
    "content-type": "image/png",
    etag: etag(PNG),
  });
});

test("getFile resolves to null for refused, missing and escaping paths", async (t) => {
  const { site, root } = projectSite(t);

  for (const refused of [
    ".env",
    "docs/index.md",
    "docs/flow.puml",
    "docs/img/missing.png",
    "../outside.png",
    "docs/../../outside.png",
    path.join(root, "outside.png"),
    "",
  ]) {
    assert.equal(await site.getFile(refused), null, refused);
  }
});

test("renderPage serves local images from /_api/files", async (t) => {
  const { site } = projectSite(t);
  const page = await site.renderPage("");
  assert.ok(page.content.includes('<img src="/_api/files/docs/img/flow.png"'), page.content);
});

test("renderPage renders a lone diagram file image through Kroki", async (t) => {
  const bodies = [];
  const kroki = createServer((req, res) => {
    let body = "";
    req.on("data", (chunk) => (body += chunk));
    req.on("end", () => {
      bodies.push(body);
      res.end('<svg width="10" height="10"><desc>rendered</desc></svg>');
    });
  });
  await new Promise((resolve) => kroki.listen(0, "127.0.0.1", resolve));
  t.after(() => {
    kroki.closeAllConnections();
    kroki.close();
  });
  const projectDir = fs.mkdtempSync(path.join(os.tmpdir(), "rw-core-diagram-"));
  t.after(() => fs.rmSync(projectDir, { recursive: true, force: true }));
  fs.mkdirSync(path.join(projectDir, "docs"));
  fs.writeFileSync(path.join(projectDir, "docs", "index.md"), "# Home\n\n![Flow](flow.puml)\n");
  fs.writeFileSync(
    path.join(projectDir, "docs", "flow.puml"),
    "@startuml\nAlice -> Bob\n@enduml\n",
  );
  const site = createSite({
    projectDir,
    diagrams: { krokiUrl: `http://127.0.0.1:${kroki.address().port}` },
  });

  const page = await site.renderPage("");

  assert.ok(page.content.includes("<desc>rendered</desc>"), page.content);
  assert.equal(bodies.length, 1);
  assert.ok(bodies[0].includes("Alice -> Bob"), bodies[0]);
  assert.equal(await site.getFile("docs/flow.puml"), null);
});

async function s3Site(t) {
  const objects = new Map([
    [
      "manifest.json",
      {
        body: JSON.stringify({
          version: 1,
          documents: [{ path: "guide", title: "Guide", has_content: true, is_dir: false }],
          source_dir: "docs",
          files: { "docs/img/flow.png": SHA256 },
        }),
        etag: '"manifest-1"',
      },
    ],
    [`files/${SHA256}.png`, { body: PNG }],
  ]);
  const server = createServer((req, res) => {
    const key = new URL(req.url, "http://localhost").pathname.replace("/bucket/docs/", "");
    const object = objects.get(key);
    if (!object) {
      res.writeHead(404);
      res.end();
      return;
    }
    res.writeHead(200, object.etag && { ETag: object.etag });
    res.end(object.body);
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  t.after(() => {
    server.closeAllConnections();
    server.close();
  });
  return createSite({
    s3: {
      bucket: "bucket",
      entity: "docs",
      endpoint: `http://127.0.0.1:${server.address().port}`,
      accessKeyId: "dummy-key",
      secretAccessKey: "dummy-secret",
    },
  });
}

test("S3 getFile serves a published file", async (t) => {
  const site = await s3Site(t);

  const file = await site.getFile("docs/img/flow.png");

  assert.deepEqual(file.content, PNG);
  assert.equal(file.contentType, "image/png");
  assert.equal(file.etag, `"${SHA256}"`);
  assert.equal(await site.getFile("docs/img/missing.png"), null);
});
