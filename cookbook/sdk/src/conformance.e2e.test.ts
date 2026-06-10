import fs from "node:fs";
import path from "node:path";

import { describe, expect, it } from "vitest";

import { LeanCtxClient } from "./client.js";
import { runConformance } from "./conformance.js";

/**
 * Live conformance run against a real lean-ctx server (GL #395).
 *
 * Driven by `scripts/sdk-conformance.sh` (CI job `sdk-conformance`): the
 * script builds the engine, starts `lean-ctx serve` and exports
 * `LEANCTX_CONFORMANCE_URL`. Without that variable the suite skips, so plain
 * `vitest run` stays hermetic.
 */
describe("runConformance E2E (real server)", () => {
  const url = process.env.LEANCTX_CONFORMANCE_URL?.trim();

  it.skipIf(!url)("all checks pass against the live /v1 surface", async () => {
    const client = new LeanCtxClient({
      baseUrl: url as string,
      bearerToken: process.env.LEANCTX_CONFORMANCE_TOKEN?.trim() || undefined,
    });
    const card = await runConformance(client);

    const matrixDir = process.env.LEANCTX_MATRIX_DIR?.trim();
    if (matrixDir) {
      fs.writeFileSync(
        path.join(matrixDir, "conformance-typescript.json"),
        JSON.stringify(
          {
            sdk: "typescript",
            passed: card.passed,
            total: card.total,
            all_passed: card.allPassed,
            checks: card.checks,
          },
          null,
          2
        )
      );
    }

    expect(
      card.checks.filter((c) => !c.passed).map((c) => `${c.name}: ${c.detail}`)
    ).toEqual([]);
    expect(card.allPassed).toBe(true);
  });
});
