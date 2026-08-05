import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { Dependency } from "rollup-plugin-license";
import { afterEach, describe, expect, it } from "vitest";
import { testing, type Attribution } from "./license-notice";

const {
  render,
  mergeAttributions,
  readLicenseText,
  readNoticeText,
  outputOnlyDependencies,
  readPackageAttribution,
  fromBundle,
  assembleAttributions,
} = testing;

// vitest runs with cwd set to this package (as `npm -w @rwdocs/viewer run
// test` does), so this is the real package.json — resolving both real
// devDependencies below against the real node_modules install.
const viewerPackageJson = join(process.cwd(), "package.json");

function attribution(
  overrides: Partial<Attribution> & Pick<Attribution, "name" | "version">,
): Attribution {
  return { license: "MIT", licenseText: "license text", ...overrides };
}

describe("mergeAttributions / render", () => {
  it("keeps two entries with the same name but different versions", () => {
    const entries = [
      attribution({ name: "pkg", version: "1.0.0", licenseText: "text-a" }),
      attribution({ name: "pkg", version: "2.0.0", licenseText: "text-b" }),
    ];

    const merged = mergeAttributions(entries);
    expect(merged).toHaveLength(2);

    const out = render(entries);
    expect(out).toContain("## pkg 1.0.0");
    expect(out).toContain("## pkg 2.0.0");
    expect(out).toContain("text-a");
    expect(out).toContain("text-b");
  });

  it("preserves noticeText when the same name@version arrives from both sources", () => {
    // Simulates the bundle source (has a NOTICE) followed by the
    // declared-dependency source for the same package — e.g.
    // approx-string-match, which is both bundled and declared. The second
    // entry lacks noticeText not because declaredDependencies can never set
    // one (it goes through readPackageAttribution, which does call
    // readNoticeText), but because approx-string-match ships no NOTICE file
    // for readNoticeText to find.
    const entries = [
      attribution({
        name: "pkg",
        version: "1.0.0",
        license: "Apache-2.0",
        noticeText: "attribution notice",
      }),
      attribution({ name: "pkg", version: "1.0.0", license: "Apache-2.0" }),
    ];

    const merged = mergeAttributions(entries);
    expect(merged).toHaveLength(1);
    expect(merged[0].noticeText).toBe("attribution notice");

    const out = render(entries);
    expect(out).toContain("### NOTICE");
    expect(out).toContain("attribution notice");
    // Only one block for the package, not two.
    expect(out.match(/^## pkg /gm)).toHaveLength(1);
  });

  it("does not collapse different versions when only one arrives twice", () => {
    const entries = [
      attribution({ name: "pkg", version: "1.0.0" }),
      attribution({ name: "pkg", version: "1.0.0" }),
      attribution({ name: "pkg", version: "2.0.0" }),
    ];
    expect(mergeAttributions(entries)).toHaveLength(2);
  });
});

describe("readLicenseText / readNoticeText", () => {
  let dir: string;

  afterEach(() => {
    if (dir) rmSync(dir, { recursive: true, force: true });
  });

  it("concatenates all license files for a dual-licensed package", () => {
    dir = mkdtempSync(join(tmpdir(), "license-notice-test-"));
    writeFileSync(join(dir, "LICENSE-MIT"), "MIT LICENSE TEXT");
    writeFileSync(join(dir, "LICENSE-APACHE"), "APACHE LICENSE TEXT");

    const text = readLicenseText(dir, "dual-licensed-pkg");
    expect(text).toContain("MIT LICENSE TEXT");
    expect(text).toContain("APACHE LICENSE TEXT");
  });

  it("matches COPYING and UNLICENSE files, not just LICENSE", () => {
    dir = mkdtempSync(join(tmpdir(), "license-notice-test-"));
    writeFileSync(join(dir, "COPYING"), "COPYING TEXT");
    writeFileSync(join(dir, "UNLICENSE"), "UNLICENSE TEXT");

    const text = readLicenseText(dir, "pkg");
    expect(text).toContain("COPYING TEXT");
    expect(text).toContain("UNLICENSE TEXT");
  });

  it("skips directory entries instead of throwing EISDIR", () => {
    dir = mkdtempSync(join(tmpdir(), "license-notice-test-"));
    mkdirSync(join(dir, "LICENSES"));
    writeFileSync(join(dir, "LICENSE"), "LICENSE TEXT");

    expect(() => readLicenseText(dir, "pkg")).not.toThrow();
    expect(readLicenseText(dir, "pkg")).toBe("LICENSE TEXT");
  });

  it("throws when no license file is present", () => {
    dir = mkdtempSync(join(tmpdir(), "license-notice-test-"));
    expect(() => readLicenseText(dir, "unlicensed-pkg")).toThrow(/no license file/);
  });

  it("reads a NOTICE file when present", () => {
    dir = mkdtempSync(join(tmpdir(), "license-notice-test-"));
    writeFileSync(join(dir, "NOTICE"), "NOTICE TEXT");
    expect(readNoticeText(dir)).toBe("NOTICE TEXT");
  });

  it("returns undefined when no NOTICE file is present", () => {
    dir = mkdtempSync(join(tmpdir(), "license-notice-test-"));
    writeFileSync(join(dir, "LICENSE"), "LICENSE TEXT");
    expect(readNoticeText(dir)).toBeUndefined();
  });
});

describe("outputOnlyDependencies", () => {
  it("attributes tailwindcss and @tailwindcss/typography even though both are devDependencies", () => {
    // Neither package appears in `dependencies`, so `declaredDependencies`
    // would miss them; both are CSS-only in the bundle Rollup sees, so
    // `fromBundle` would miss them too. This is the hand-maintained list's
    // whole reason to exist.
    const entries = outputOnlyDependencies(viewerPackageJson);

    expect(entries.map((e) => e.name).sort()).toEqual(["@tailwindcss/typography", "tailwindcss"]);
    for (const entry of entries) {
      expect(entry.version).toBeTruthy();
      expect(entry.license).toBeTruthy();
      expect(entry.licenseText.length).toBeGreaterThan(0);
    }
  });
});

describe("readPackageAttribution", () => {
  let dir: string;

  afterEach(() => {
    if (dir) rmSync(dir, { recursive: true, force: true });
  });

  it("throws when a package's package.json declares no license", () => {
    dir = mkdtempSync(join(tmpdir(), "license-notice-test-"));
    const pkgDir = join(dir, "node_modules", "no-license-pkg");
    mkdirSync(pkgDir, { recursive: true });
    writeFileSync(join(pkgDir, "package.json"), JSON.stringify({ version: "1.0.0" }));

    expect(() => readPackageAttribution("no-license-pkg", [dir])).toThrow(/declares no license/);
  });
});

describe("fromBundle", () => {
  it("throws when a bundle dependency has no licenseText", () => {
    const deps = [
      {
        name: "pkg-without-license-text",
        version: "1.0.0",
        license: "MIT",
        licenseText: null,
      } as Dependency,
    ];

    expect(() => fromBundle(deps)).toThrow(/incomplete metadata/);
  });

  // A null name would otherwise be coerced to `""` and key the merge map at
  // `"@"`, where a second nameless package overwrites the first — the silent
  // drop this module exists to prevent.
  it.each(["name", "version"])("throws when a bundle dependency has no %s", (field) => {
    const deps = [
      {
        name: "pkg",
        version: "1.0.0",
        license: "MIT",
        licenseText: "MIT text",
        [field]: null,
      } as unknown as Dependency,
    ];

    expect(() => fromBundle(deps)).toThrow(/incomplete metadata/);
  });
});

describe("assembleAttributions", () => {
  it("includes entries from all three sources: bundle, declared, and output-only", () => {
    // A fake bundle dependency stands in for `fromBundle` — it needs no real
    // install, unlike the other two sources, which resolve against the real
    // viewer package.json so @fontsource/* (declared) and tailwindcss
    // (output-only) genuinely resolve from node_modules.
    const bundleDeps = [
      {
        name: "bundle-only-pkg",
        version: "9.9.9",
        license: "MIT",
        licenseText: "bundle license text",
      } as Dependency,
    ];

    const entries = assembleAttributions(bundleDeps, viewerPackageJson);
    const names = entries.map((e) => e.name);

    expect(names).toContain("bundle-only-pkg"); // fromBundle
    expect(names).toContain("@fontsource/roboto"); // declaredDependencies
    expect(names).toContain("@fontsource/jetbrains-mono"); // declaredDependencies
    expect(names).toContain("tailwindcss"); // outputOnlyDependencies
    expect(names).toContain("@tailwindcss/typography"); // outputOnlyDependencies
  });
});
