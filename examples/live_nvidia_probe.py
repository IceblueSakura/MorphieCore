"""NVIDIA boundary selection; requires an existing shared MORPHIECORE_PROBE_RUN.

Plan offline with examples/probe.py. See docs/probes.md for budgets and gates.
"""

from probe_support.entry import entry

if __name__ == "__main__":
    try:
        raise SystemExit(entry("nvidia"))
    except Exception:
        print(
            "NVIDIA probe stopped; inspect the run ledger. Private details suppressed."
        )
        raise SystemExit(1)
