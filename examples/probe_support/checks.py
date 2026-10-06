"""Explicit checks survive optimized Python and never include payload values."""

import signal


def install_signals():
    def terminate(signum, frame):
        raise SystemExit(128 + signum)

    signal.signal(signal.SIGTERM, terminate)


class ProbeFailure(RuntimeError):
    def __init__(self, code, kind="oracle"):
        self.code, self.kind = code, kind
        super().__init__("probe check failed")


def require(condition, code="oracle", kind="oracle"):
    if not condition:
        raise ProbeFailure(code, kind)
