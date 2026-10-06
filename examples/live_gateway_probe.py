"""GPT-6 Luna normal/opaque-history selection over the shared bounded runner."""

from probe_support.entry import entry

if __name__ == "__main__":
    try:
        raise SystemExit(entry("gateway"))
    except Exception:
        print(
            "Gateway probe stopped; inspect the run ledger. Private details suppressed."
        )
        raise SystemExit(1)
