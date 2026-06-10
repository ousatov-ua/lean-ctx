"""Live conformance run against a real lean-ctx server (GL #395).

Driven by ``scripts/sdk-conformance.sh`` (CI job ``sdk-conformance``): the
script builds the engine, starts ``lean-ctx serve`` and exports
``LEANCTX_CONFORMANCE_URL``. Without that variable the suite skips, so plain
``pytest`` runs stay hermetic.
"""

from __future__ import annotations

import json
import os
import pathlib

import pytest

from leanctx import LeanCtxClient, run_conformance

URL = os.environ.get("LEANCTX_CONFORMANCE_URL", "").strip()


@pytest.mark.skipif(not URL, reason="LEANCTX_CONFORMANCE_URL not set")
def test_live_conformance_all_checks_pass() -> None:
    client = LeanCtxClient(
        URL, bearer_token=os.environ.get("LEANCTX_CONFORMANCE_TOKEN") or None
    )
    card = run_conformance(client)

    matrix_dir = os.environ.get("LEANCTX_MATRIX_DIR", "").strip()
    if matrix_dir:
        out = pathlib.Path(matrix_dir) / "conformance-python.json"
        out.write_text(
            json.dumps(
                {
                    "sdk": "python",
                    "passed": card.passed,
                    "total": card.total,
                    "all_passed": card.all_passed,
                    "checks": [
                        {"name": c.name, "passed": c.passed, "detail": c.detail}
                        for c in card.checks
                    ],
                },
                indent=2,
            )
        )

    failed = [f"{c.name}: {c.detail}" for c in card.checks if not c.passed]
    assert card.all_passed, failed
