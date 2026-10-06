"""Own one isolated Pi SDK probe, using the same run ledger as Python/native gates."""

import argparse
import contextlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import uuid
from probe_support.checks import require, install_signals
from probe_support.ledger import Run, MODELS
from probe_support.catalog import select_protocol
from probe_support.runtime import gateway, ROOT


def main():
    install_signals()
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run", default=os.environ.get("MORPHIECORE_PROBE_RUN"))
    parser.add_argument(
        "--package", required=True, help="Explicit fixed Pi package directory (1.0.4)"
    )
    parser.add_argument("--model", default="nemotron-3-super", choices=MODELS)
    parser.add_argument("--protocol", choices=("chat", "responses"))
    parser.add_argument("--thinking", choices=("off", "minimal"), default="off")
    parser.add_argument("--live", action="store_true")
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--invalid-auth", action="store_true")
    args = parser.parse_args()
    protocol = select_protocol(args.model, args.protocol)
    require(
        args.live != args.check and (not args.invalid_auth or args.check),
        "mode",
        "setup",
    )
    with contextlib.ExitStack() as stack:
        if args.run:
            run = Run(args.run)
        else:
            require(args.check, "live_requires_plan", "setup")
            temp = stack.enter_context(tempfile.TemporaryDirectory())
            run = Run.create(
                Path(temp) / "run", providers=MODELS[args.model][0], limit=2
            )
        proxy = None
        reject = None
        if args.check:

            class Reject(BaseHTTPRequestHandler):
                count = 0

                def do_CONNECT(self):
                    type(self).count += 1
                    self.send_error(502)

                def log_message(self, *_):
                    pass

            reject = ThreadingHTTPServer(("127.0.0.1", 0), Reject)
            threading.Thread(target=reject.serve_forever, daemon=True).start()
            stack.callback(reject.server_close)
            stack.callback(reject.shutdown)
            proxy = f"http://127.0.0.1:{reject.server_port}"
        else:
            os.environ["MORPHIECORE_PROBE_LIVE"] = "1"
        with gateway(run, [args.model], synthetic=args.check, proxy=proxy) as (
            origin,
            key,
        ):
            env = {
                name: os.environ[name]
                for name in ("PATH", "HOME", "LANG", "TERM")
                if name in os.environ
            }
            home = run.directory / f"pi-{uuid.uuid4().hex}"
            env.update(
                MORPHIECORE_PROBE_RUN=str(run.directory),
                MORPHIECORE_PI_HOME=str(home),
                MORPHIECORE_PI_PACKAGE=str(Path(args.package).resolve()),
                MORPHIECORE_PROBE_PYTHON=sys.executable,
                MORPHIECORE_TEST_MODEL=args.model,
                MORPHIECORE_TEST_PROTOCOL=protocol,
                MORPHIECORE_TEST_THINKING=args.thinking,
                MORPHIECORE_TEST_MODE="check" if args.check else "live",
                MORPHIECORE_TEST_UPSTREAM=origin,
                MORPHIECORE_CLIENT_KEY=key,
                PI_CODING_AGENT_DIR=str(home),
                PI_OFFLINE="1",
                PI_TELEMETRY="0",
                PI_SKIP_VERSION_CHECK="1",
                NO_PROXY="127.0.0.1,localhost",
            )
            if args.invalid_auth:
                env["MORPHIECORE_TEST_INVALID_AUTH"] = "1"
            child = subprocess.Popen(
                ["node", str(ROOT / "examples/pi_probe.ts")], env=env, cwd=ROOT
            )
            try:
                code = child.wait(timeout=400)
            finally:
                if child.poll() is None:
                    child.terminate()
                    try:
                        child.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        child.kill()
                        child.wait()
        if args.check:
            require(
                Reject.count == (0 if args.invalid_auth else 2),
                "synthetic_egress_count",
                "wire",
            )
            print(f"synthetic_proxy_attempts={Reject.count}")
        return code


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception:
        print("Pi probe stopped; private details suppressed.", file=sys.stderr)
        raise SystemExit(1)
