import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";

const installer = readFileSync(new URL("./install-cargo-codspeed.sh", import.meta.url), "utf8");
const pins = [
  ...installer.matchAll(
    /url="(?<url>https:\/\/github\.com\/CodSpeedHQ\/codspeed-rust\/releases\/download\/(?<version>v[\d.]+)\/cargo-codspeed-(?<arch>x86_64|aarch64)-unknown-linux-musl)"\s+sha256="(?<sha256>[a-f0-9]{64})"/g,
  ),
].map((match) => match.groups);

test("both architectures pin the same release with distinct SHA-256 checksums", () => {
  assert.equal(pins.length, 2);
  assert.deepEqual(pins.map((pin) => pin.arch).sort(), ["aarch64", "x86_64"]);
  assert.equal(new Set(pins.map((pin) => pin.version)).size, 1);
  assert.equal(new Set(pins.map((pin) => pin.sha256)).size, 2);
});

const fixture =
  '#!/usr/bin/env bash\nprintf "%s\\n" "$*" >> "$EXECUTION_LOG"\necho "verified fixture"\n';
const digest = createHash("sha256").update(fixture).digest("hex");
const stale = '#!/usr/bin/env bash\necho stale >> "$EXECUTION_LOG"\n';
const existingPath = "/existing/bin\n";

function setup(t, { fallback = false, cached = false } = {}) {
  const root = mkdtempSync(join(tmpdir(), "codspeed-test-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const home = join(root, "home");
  const cargoHome = fallback ? join(home, ".cargo") : join(root, "custom-cargo");
  const bin = join(cargoHome, "bin");
  const binary = join(bin, "cargo-codspeed");
  const tools = join(root, "tools");
  const downloads = join(root, "downloads");
  for (const dir of [home, tools, downloads]) mkdirSync(dir);
  const script = join(root, "installer.sh");
  assert.equal([...installer.matchAll(/sha256="[a-f0-9]{64}"/g)].length, 2);
  writeFileSync(script, installer.replace(/sha256="[a-f0-9]{64}"/g, `sha256="${digest}"`));
  const payload = join(root, "payload");
  writeFileSync(payload, fixture);
  const githubPath = join(root, "github-path");
  writeFileSync(githubPath, existingPath);
  const curlLog = join(root, "curl-log");
  const executionLog = join(root, "execution-log");
  writeFileSync(
    join(tools, "curl"),
    `#!/usr/bin/env bash
set -euo pipefail
url= output=
while (($#)); do
  case "$1" in
    --output) output=$2; shift 2 ;;
    https://*) url=$1; shift ;;
    *) shift ;;
  esac
done
printf '%s\\n' "$url" >> "$CURL_LOG"
if [[ "\${FAIL_DOWNLOAD:-0}" == 1 ]]; then
  printf partial > "$output"
  exit 22
fi
cat "$PAYLOAD" > "$output"
`,
    { mode: 0o755 },
  );
  if (cached) {
    mkdirSync(bin, { recursive: true });
    writeFileSync(binary, stale, { mode: 0o755 });
  }
  const env = {
    PATH: `${tools}:${process.env.PATH}`,
    HOME: home,
    RUNNER_OS: "Linux",
    RUNNER_ARCH: "X64",
    RUNNER_TEMP: downloads,
    GITHUB_PATH: githubPath,
    CURL_LOG: curlLog,
    EXECUTION_LOG: executionLog,
    PAYLOAD: payload,
    ...(fallback ? {} : { CARGO_HOME: cargoHome }),
  };
  return {
    binary,
    bin,
    home,
    payload,
    githubPath,
    curlLog,
    executionLog,
    run(overrides = {}) {
      const result = spawnSync("bash", [script], {
        env: { ...env, ...overrides },
        encoding: "utf8",
      });
      assert.ifError(result.error);
      assert.equal(result.signal, null);
      assert.deepEqual(readdirSync(downloads), [], "temporary downloads must be cleaned up");
      return result;
    },
    assertRejected(result) {
      assert.notEqual(result.status, 0, result.stdout + result.stderr);
      assert.equal(existsSync(executionLog), false);
      assert.equal(readFileSync(githubPath, "utf8"), existingPath);
      if (cached) assert.equal(readFileSync(binary, "utf8"), stale);
      else assert.equal(existsSync(binary), false);
    },
  };
}

for (const [arch, target] of [
  ["X64", "x86_64"],
  ["ARM64", "aarch64"],
]) {
  test(`${arch}: verified download replaces cached binary and updates GITHUB_PATH`, (t) => {
    const h = setup(t, { cached: true });
    const result = h.run({ RUNNER_ARCH: arch });
    assert.equal(result.status, 0, result.stdout + result.stderr);
    assert.equal(
      readFileSync(h.curlLog, "utf8"),
      `${pins.find((pin) => pin.arch === target).url}\n`,
    );
    assert.equal(readFileSync(h.binary, "utf8"), fixture);
    assert.equal(statSync(h.binary).mode & 0o777, 0o755);
    assert.equal(existsSync(h.executionLog), false);
    assert.equal(readFileSync(h.githubPath, "utf8"), `${existingPath}${h.bin}\n`);
    assert.equal(existsSync(join(h.home, ".cargo")), false);
  });
}

test("falls back to HOME/.cargo when CARGO_HOME is unset", (t) => {
  const h = setup(t, { fallback: true });
  const result = h.run();
  assert.equal(result.status, 0, result.stdout + result.stderr);
  assert.equal(readFileSync(h.binary, "utf8"), fixture);
  assert.equal(existsSync(h.executionLog), false);
  assert.equal(
    readFileSync(h.githubPath, "utf8"),
    `${existingPath}${join(h.home, ".cargo/bin")}\n`,
  );
});

for (const cached of [false, true]) {
  test(`corrupt download is rejected before installation (cached=${cached})`, (t) => {
    const h = setup(t, { cached });
    writeFileSync(h.payload, `${fixture}# corrupt bytes\n`);
    const result = h.run();
    h.assertRejected(result);
    assert.match(result.stdout + result.stderr, /FAILED/);
    assert.equal(existsSync(h.curlLog), true);
  });
}

test("failed partial download is rejected and cleaned up", (t) => {
  const h = setup(t);
  const result = h.run({ FAIL_DOWNLOAD: "1" });
  h.assertRejected(result);
  assert.equal(result.status, 22);
  assert.equal(existsSync(h.curlLog), true);
});

for (const overrides of [
  { RUNNER_OS: "macOS" },
  { RUNNER_OS: "" },
  { RUNNER_ARCH: "ARM" },
  { RUNNER_ARCH: "" },
]) {
  test(`unsupported runner fails before curl: ${JSON.stringify(overrides)}`, (t) => {
    const h = setup(t);
    h.assertRejected(h.run(overrides));
    assert.equal(existsSync(h.curlLog), false);
  });
}
