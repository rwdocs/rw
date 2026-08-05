import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import license, { type Dependency } from "rollup-plugin-license";
import type { Plugin } from "vite";

/** One attributed package, from any of the three sources. */
export interface Attribution {
  name: string;
  version: string;
  license: string;
  licenseText: string;
  noticeText?: string;
}

/** A package's directory and its already-read, not-yet-parsed `package.json`. */
interface PackageLocation {
  dir: string;
  packageJson: string;
}

/**
 * Locates a package directory by path rather than Node resolution, which an
 * `exports` map can block for `package.json`. Workspace dependencies may be
 * hoisted, so the repository root is searched as well as the workspace itself.
 *
 * Returns the `package.json` text it read to confirm the package exists, so
 * `readPackageAttribution` doesn't read the same file a second time.
 *
 * Only a missing package under a given root (`ENOENT`) falls through to the
 * next root; anything else — `EACCES`, `EMFILE`, a directory where a file was
 * expected — is a real failure and must not be misreported as "not installed
 * under any root".
 */
function packageDir(name: string, roots: string[]): PackageLocation {
  for (const root of roots) {
    const dir = join(root, "node_modules", name);
    try {
      const packageJson = readFileSync(join(dir, "package.json"), "utf8");
      return { dir, packageJson };
    } catch (err) {
      if ((err as NodeJS.ErrnoException).code !== "ENOENT") {
        throw err;
      }
    }
  }
  throw new Error(`license notice: cannot locate ${name} under any node_modules`);
}

/**
 * Reads every file in `dir` matching `pattern`, sorted for deterministic
 * ordering, and concatenates their contents. `readdirSync` gives no ordering
 * guarantee and directory entries (e.g. a `LICENSES/` folder) would throw an
 * opaque `EISDIR` if read as a file, so both are handled here once for reuse
 * by license- and notice-file lookups.
 */
function readMatchingFiles(dir: string, pattern: RegExp): string[] {
  return readdirSync(dir, { withFileTypes: true })
    .filter((entry) => entry.isFile() && pattern.test(entry.name))
    .map((entry) => entry.name)
    .sort()
    .map((name) => readFileSync(join(dir, name), "utf8").trim());
}

const LICENSE_FILE_PATTERN = /^(licen[cs]e|copying|unlicense)/i;
const NOTICE_FILE_PATTERN = /^notice/i;

/**
 * Reads every license file in `dir`, not just the first match. A dual-licensed
 * package (e.g. `LICENSE-MIT` + `LICENSE-APACHE`, this repo's own layout)
 * ships two files for one SPDX expression like `MIT OR Apache-2.0`; picking
 * only one silently drops half the stated license.
 */
function readLicenseText(dir: string, name: string): string {
  const texts = readMatchingFiles(dir, LICENSE_FILE_PATTERN);
  if (texts.length === 0) {
    throw new Error(`license notice: no license file in ${name} (${dir})`);
  }
  return texts.join("\n\n---\n\n");
}

/** Reads the NOTICE file in `dir`, if one exists — Apache-2.0's redistribution requirement. */
function readNoticeText(dir: string): string | undefined {
  const texts = readMatchingFiles(dir, NOTICE_FILE_PATTERN);
  return texts.length > 0 ? texts.join("\n\n---\n\n") : undefined;
}

/** Reads one package's license metadata from its installed directory. */
function readPackageAttribution(name: string, roots: string[]): Attribution {
  const { dir, packageJson } = packageDir(name, roots);
  const dep = JSON.parse(packageJson) as {
    version: string;
    license?: string;
  };
  if (!dep.license) {
    throw new Error(`license notice: ${name} declares no license`);
  }
  return {
    name,
    version: dep.version,
    license: dep.license,
    licenseText: readLicenseText(dir, name),
    noticeText: readNoticeText(dir),
  };
}

/**
 * The `node_modules` roots to search for a package installed alongside the
 * viewer: the viewer's own workspace, then the repository root, since a
 * dependency may be hoisted there instead.
 */
function packageRoots(viewerPackageJson: string): string[] {
  const viewerRoot = dirname(viewerPackageJson);
  return [viewerRoot, join(viewerRoot, "..", "..")];
}

/**
 * Packages declared as runtime dependencies of the viewer.
 *
 * rollup-plugin-license reports only packages whose JavaScript entered the
 * bundle — it gates on `renderedLength > 0` in the JS chunk, and Vite extracts
 * CSS into a separate asset. That makes CSS-only packages invisible to it,
 * including the `@fontsource` fonts, which are OFL-1.1 and ship as `.woff2`
 * assets. They are declared dependencies, so they are read directly here.
 *
 * This only walks direct dependencies of `viewerPackageJson`. A transitive
 * CSS-only package — one pulled in by a direct dependency rather than
 * declared here — would not be attributed by any of the three sources. That
 * is an accepted limitation of this design, not a bug to fix here.
 */
function declaredDependencies(viewerPackageJson: string): Attribution[] {
  const manifest = JSON.parse(readFileSync(viewerPackageJson, "utf8")) as {
    dependencies?: Record<string, string>;
  };
  const roots = packageRoots(viewerPackageJson);

  return Object.keys(manifest.dependencies ?? {}).map((name) =>
    readPackageAttribution(name, roots),
  );
}

/**
 * Build tools whose *output* — not the tool itself — ships inside the
 * bundle, even though the tool is a devDependency rather than a runtime one.
 *
 * Neither other source catches these. `fromBundle` (rollup-plugin-license)
 * gates on JS `renderedLength`, so CSS-only output is invisible to it just
 * like the fonts above. `declaredDependencies` only walks `dependencies`, and
 * a build tool correctly belongs in `devDependencies` — it isn't imported by
 * the shipped code, only run against it. Tailwind and its typography plugin
 * both land here: their `preflight` reset and ~274 `prose` rules appear
 * verbatim in the generated stylesheet.
 *
 * Unlike the other two sources, this list is hand-maintained — "which
 * devDependency's output ends up in the bundle" is not a fact any manifest
 * records. A package that stops shipping but stays installed keeps being
 * attributed here, silently, until someone deletes the entry.
 */
const OUTPUT_ONLY_DEV_DEPENDENCIES = ["tailwindcss", "@tailwindcss/typography"];

function outputOnlyDependencies(viewerPackageJson: string): Attribution[] {
  const roots = packageRoots(viewerPackageJson);

  return OUTPUT_ONLY_DEV_DEPENDENCIES.map((name) => readPackageAttribution(name, roots));
}

/**
 * Throws rather than emitting a partial document: a notice that presents
 * itself as complete while silently dropping a dependency is worse than no
 * notice, because consumers treat it as the full list.
 */
function fromBundle(dependencies: Dependency[]): Attribution[] {
  const unresolved = dependencies.filter(
    (d) => !d.name || !d.version || !d.license || !d.licenseText,
  );
  if (unresolved.length > 0) {
    const names = unresolved.map((d) => `${d.name}@${d.version}`).join(", ");
    throw new Error(
      `license notice: incomplete metadata for ${names}. ` +
        `Add the missing fields rather than removing this check.`,
    );
  }
  return dependencies.map((d) => ({
    name: d.name ?? "",
    version: d.version ?? "",
    license: d.license ?? "",
    licenseText: d.licenseText ?? "",
    noticeText: d.noticeText ?? undefined,
  }));
}

/**
 * Unions entries from all sources, keyed on `name@version` — not just
 * `name` — because `multipleVersions: true` deliberately hands the bundle
 * source two entries for a package bundled at two versions, and those must
 * both survive. When the same `name@version` arrives from more than one
 * source (a package that is both bundled and declared, e.g.
 * `approx-string-match`), the later entry wins for every field except
 * `noticeText`, which is kept from whichever source set it. `fromBundle` and
 * `readPackageAttribution` (used by both `declaredDependencies` and
 * `outputOnlyDependencies`) each look for a NOTICE file independently, so one
 * source can find it while another comes back without one; falling back
 * instead of overwriting stops a later, notice-less entry from silently
 * dropping an Apache-2.0 NOTICE an earlier entry found.
 */
function mergeAttributions(entries: Attribution[]): Attribution[] {
  const merged = new Map<string, Attribution>();
  for (const entry of entries) {
    const key = `${entry.name}@${entry.version}`;
    const existing = merged.get(key);
    merged.set(
      key,
      existing ? { ...entry, noticeText: entry.noticeText ?? existing.noticeText } : entry,
    );
  }
  return [...merged.values()];
}

function render(entries: Attribution[]): string {
  const sorted = mergeAttributions(entries).sort(
    (a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version),
  );

  const blocks = sorted.map((e) => {
    const notice = e.noticeText ? `\n\n### NOTICE\n\n\`\`\`\n${e.noticeText.trim()}\n\`\`\`` : "";
    return (
      `## ${e.name} ${e.version}\n\n` +
      `SPDX-License-Identifier: ${e.license}\n\n` +
      `\`\`\`\n${e.licenseText.trim()}\n\`\`\`${notice}`
    );
  });

  return (
    `# Third-Party Licenses (JavaScript)\n\n` +
    `The following packages are attributed for this artifact: packages whose ` +
    `JavaScript was bundled here, packages declared as runtime dependencies ` +
    `of \`@rwdocs/viewer\` (some may not be part of this particular bundle — ` +
    `listing an unused one is harmless; omitting a used one is not), and ` +
    `build tools whose generated output ships here even though the tool ` +
    `itself is a devDependency. Each is listed with the license text ` +
    `supplied by its author.\n\n` +
    `${blocks.join("\n\n")}\n`
  );
}

/**
 * Assembles the full attribution list from all three sources: packages whose
 * JS entered the bundle, the viewer's declared runtime dependencies, and the
 * hand-maintained output-only build-tool list. Dropping a source here is
 * invisible to any test that exercises the source functions individually.
 */
function assembleAttributions(
  dependencies: Dependency[],
  viewerPackageJson: string,
): Attribution[] {
  return [
    ...fromBundle(dependencies),
    ...declaredDependencies(viewerPackageJson),
    ...outputOnlyDependencies(viewerPackageJson),
  ];
}

/** Emits the notice for everything the viewer ships to `outFile`. */
export function licenseNotice(outFile: string, viewerPackageJson: string): Plugin {
  return license({
    thirdParty: {
      includePrivate: false,
      includeSelf: false,
      multipleVersions: true,
      output: {
        file: outFile,
        encoding: "utf-8",
        template: (dependencies: Dependency[]) =>
          render(assembleAttributions(dependencies, viewerPackageJson)),
      },
    },
  }) as Plugin;
}

export const testing = {
  render,
  mergeAttributions,
  outputOnlyDependencies,
  readLicenseText,
  readNoticeText,
  readPackageAttribution,
  fromBundle,
  assembleAttributions,
};
