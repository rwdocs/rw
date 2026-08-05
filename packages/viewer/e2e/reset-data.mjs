import { rm } from "node:fs/promises";
import path from "node:path";

/**
 * Delete the comment database every e2e `rw serve` shares — `rw` puts it beside
 * the config, and all four fixture configs sit in `e2e/fixtures/`.
 *
 * Must run before any server boots: `rw serve` opens the database eagerly and
 * keeps its descriptors, so a later delete resets nothing and the run proceeds
 * against the unlinked copy. Playwright's `globalSetup` cannot host this — it
 * runs after `webServer` is up. Nor does this reach a server started by hand,
 * which `reuseExistingServer` adopts rather than restarts.
 */
const dataDir = path.join(import.meta.dirname, "fixtures", ".rw");
try {
  await rm(dataDir, { recursive: true, force: true });
} catch (e) {
  // Never soften this to a warning: a silent pass runs the suite against
  // un-reset state. `EBUSY`/`EPERM` usually mean a process still holds the
  // database open, which Windows will not let anyone unlink — but `EPERM` can
  // also be permissions, so the message suggests rather than asserts.
  if (e.code !== "EBUSY" && e.code !== "EPERM") throw e;
  throw new Error(
    `Cannot reset the e2e data directory: ${dataDir}\n` +
      `If an \`rw serve\` is running against these fixtures, stop it and re-run — Windows ` +
      `will not unlink a file another process holds open. Otherwise check the directory's ` +
      `permissions and flags.`,
    { cause: e },
  );
}
