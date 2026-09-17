import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { createServer } from "node:http";
import { crc32 } from "node:zlib";

// Only the objects/operations used by the attrs addon test, not an S3 emulator.
export async function attrsS3Fixture(t, attrs) {
  const objects = new Map();
  const requests = [];
  const failures = [];
  const prefix = "/attrs-test/docs/";
  const cacheKeys = new Set([
    "cache/site/structure",
    "cache/pages/selected",
    "cache/pages/unrelated",
  ]);
  const put = (key, value, cacheEtag) => {
    const body = Buffer.isBuffer(value) ? value : Buffer.from(JSON.stringify(value));
    objects.set(key, {
      body,
      etag: `"${createHash("sha256").update(body).digest("hex")}"`,
      cacheEtag,
    });
  };
  const setAttrs = (next) => {
    put("manifest.json", {
      version: 1,
      documents: [
        {
          path: "selected",
          title: "Selected",
          has_content: true,
          is_dir: false,
          ...(Object.keys(next).length ? { attrs: next } : {}),
        },
        { path: "unrelated", title: "Unrelated", has_content: true, is_dir: false },
        {
          path: "virtual",
          title: "Virtual",
          has_content: false,
          is_dir: true,
          ...(Object.keys(next).length ? { attrs: next } : {}),
        },
      ],
      mtimes: { selected: 1700000000, unrelated: 1700000000, virtual: 1700000000 },
    });
  };
  setAttrs(attrs);
  put("pages/selected.json", { content: "# Selected\n\nFixed body.\n" });
  put("pages/unrelated.json", { content: "# Unrelated\n\nOther fixed body.\n" });

  const server = createServer(async (req, res) => {
    try {
      const url = new URL(req.url, "http://127.0.0.1");
      assert.ok(url.pathname.startsWith(prefix), `unexpected path: ${req.url}`);
      const key = url.pathname.slice(prefix.length);
      requests.push(`${req.method} ${key}`);
      const operation = { GET: "GetObject", HEAD: "HeadObject", PUT: "PutObject" }[req.method];
      assert.ok(operation, `unsupported method: ${req.method}`);
      for (const [name, value] of url.searchParams) {
        assert.equal(name, "x-id", `unsupported query: ${req.url}`);
        assert.equal(value, operation);
      }
      assert.match(req.headers.authorization ?? "", /Credential=rw-attrs-dummy\//);
      if (req.method === "PUT") {
        assert.ok(cacheKeys.has(key), `unsupported PUT: ${key}`);
        const body = await decodeUpload(req);
        // Invalid framing must never masquerade as an ordinary cache miss.
        assert.ok(JSON.parse(body.toString()), "cache upload must be decoded JSON");
        const cacheEtag = req.headers["x-amz-meta-cache-etag"];
        assert.equal(typeof cacheEtag, "string");
        assert.ok(cacheEtag.length > 0);
        put(key, body, cacheEtag);
        res.writeHead(200, { ETag: objects.get(key).etag, "Content-Length": 0 });
        res.end();
        return;
      }
      assert.ok(objects.has(key) || cacheKeys.has(key), `unsupported GET: ${key}`);
      if (req.method === "HEAD") assert.equal(key, "manifest.json");
      const object = objects.get(key);
      if (!object) {
        const body = "<Error><Code>NoSuchKey</Code><Message>Fixture cache miss</Message></Error>";
        res.writeHead(404, {
          "Content-Type": "application/xml",
          "Content-Length": Buffer.byteLength(body),
        });
        res.end(body);
        return;
      }
      res.writeHead(200, {
        ETag: object.etag,
        "Content-Type": "application/octet-stream",
        "Content-Length": object.body.length,
        ...(object.cacheEtag ? { "x-amz-meta-cache-etag": object.cacheEtag } : {}),
      });
      res.end(req.method === "HEAD" ? undefined : object.body);
    } catch (error) {
      failures.push(error.stack);
      res.writeHead(400, { "Content-Length": 0 });
      res.end();
    }
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  t.after(async () => {
    await new Promise((resolve, reject) => {
      server.close((error) => (error ? reject(error) : resolve()));
      server.closeAllConnections();
    });
    assert.deepEqual(failures, [], "unsupported S3 request or malformed upload");
  });
  return {
    config: {
      s3: {
        bucket: "attrs-test",
        entity: "docs",
        region: "us-east-1",
        endpoint: `http://127.0.0.1:${server.address().port}`,
        accessKeyId: "rw-attrs-dummy",
        secretAccessKey: "rw-attrs-dummy-secret",
      },
    },
    setAttrs,
    objects,
    takeRequests() {
      assert.deepEqual(failures, [], "unsupported S3 request or malformed upload");
      return requests.splice(0);
    },
  };
}

async function decodeUpload(req) {
  const chunks = [];
  for await (const chunk of req) chunks.push(chunk);
  const wire = Buffer.concat(chunks);
  // Node removes HTTP transfer-encoding, but NOT AWS's content-encoding.
  assert.equal(req.headers["x-amz-sdk-checksum-algorithm"], "CRC32");
  let body = wire;
  let checksum = req.headers["x-amz-checksum-crc32"];
  if (req.headers["content-encoding"] === "aws-chunked") {
    assert.equal(req.headers["x-amz-trailer"], "x-amz-checksum-crc32");
    const decoded = [];
    let offset = 0;
    while (true) {
      const end = wire.indexOf("\r\n", offset);
      assert.ok(end >= offset, "missing AWS chunk length");
      const size = wire.subarray(offset, end).toString();
      assert.match(size, /^[0-9a-f]+$/i, "unsupported signed AWS chunk");
      offset = end + 2;
      const length = Number.parseInt(size, 16);
      if (length === 0) break;
      assert.ok(offset + length + 2 <= wire.length, "truncated AWS chunk");
      decoded.push(wire.subarray(offset, offset + length));
      offset += length;
      assert.equal(wire.subarray(offset, offset + 2).toString(), "\r\n");
      offset += 2;
    }
    const trailer = wire.subarray(offset).toString();
    const match = /^x-amz-checksum-crc32:([A-Za-z0-9+/=]+)\r\n\r\n$/.exec(trailer);
    assert.ok(match, `unsupported AWS trailer: ${JSON.stringify(trailer)}`);
    checksum = match[1];
    body = Buffer.concat(decoded);
    assert.equal(body.length, Number(req.headers["x-amz-decoded-content-length"]));
  } else {
    assert.equal(req.headers["content-encoding"], undefined);
  }
  const expected = Buffer.alloc(4);
  expected.writeUInt32BE(crc32(body));
  assert.equal(checksum, expected.toString("base64"), "AWS CRC32 must match decoded bytes");
  return body;
}
