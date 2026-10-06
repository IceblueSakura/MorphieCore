"""Provider matrix selection over the shared bounded runner; Kimi remains paused."""

from probe_support.entry import entry

if __name__ == "__main__":
    try:
        raise SystemExit(entry("matrix"))
    except Exception:
        print(
            "Provider matrix stopped; inspect the run ledger. Private details suppressed."
        )
        raise SystemExit(1)
