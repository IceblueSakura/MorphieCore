#!/usr/bin/env python3
"""Offline plan/control CLI and explicit live probe entry. No secrets in arguments."""

import argparse
import json
import os
import sys
from probe_support.ledger import Run
from probe_support.catalog import select_bindings, select_image_bindings
from probe_support.checks import require, install_signals


def main():
    install_signals()
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    plan = sub.add_parser("plan")
    plan.add_argument("directory")
    plan.add_argument("--providers", default="nvidia")
    plan.add_argument(
        "--model", action="append", help="Narrow the selected providers to exact models"
    )
    plan.add_argument("--limit", type=int, default=32)
    plan.add_argument("--tokens", type=int)
    plan.add_argument("--task", choices=["generation", "images"], default="generation")
    plan.add_argument("--images-per-request", type=int, choices=range(1, 11))
    plan.add_argument("--continue-oracle", action="store_true")
    plan.add_argument("--dry-run", action="store_true")
    run = sub.add_parser("run")
    run.add_argument("directory")
    run.add_argument("--live", action="store_true")
    run.add_argument("--model", action="append")
    run.add_argument("--cases")
    run.add_argument("--protocol", choices=["chat", "responses", "images"])
    run.add_argument("--delivery", choices=["json", "sse"])
    run.add_argument("--effort", choices=["none", "minimal", "medium", "max"])
    run.add_argument("--dry-run", action="store_true")
    show = sub.add_parser("report")
    show.add_argument("directory")
    ctl = sub.add_parser("control")
    ctl.add_argument("directory")
    ctl.add_argument(
        "action", choices=["reserve", "dispatched", "finish", "register", "check"]
    )
    args = parser.parse_args()
    if args.command == "plan":
        image_task = args.task == "images"
        require(args.images_per_request is None or image_task, "plan_task", "setup")
        rows = select_image_bindings(args.providers, args.model) if image_task else select_bindings(args.providers, models=args.model)
        tokens = args.tokens if image_task else (2048 if args.tokens is None else args.tokens)
        require(1 <= args.limit <= 256 and (tokens is None if image_task else 1 <= tokens <= 2048), "plan_budget", "setup")
        if args.dry_run:
            print(
                json.dumps(
                    {
                        "models": [row[1] for row in rows],
                        "limit": args.limit,
                        "tokens": tokens,
                        **({"images_per_request": args.images_per_request or 1} if image_task else {}),
                    }
                )
            )
            return 0
        created = Run.create(
            args.directory,
            providers=args.providers,
            models=args.model,
            limit=args.limit,
            tokens=tokens,
            task=args.task,
            images_per_request=args.images_per_request,
            continue_oracle=args.continue_oracle,
        )
        print(json.dumps({"run": str(created.directory), "id": created.plan["id"]}))
        return 0
    ledger = Run(args.directory)
    if args.command == "control":
        raw = sys.stdin.buffer.read(16385)
        require(len(raw) <= 16384, "control_bytes", "setup")
        data = json.loads(raw)
        if args.action == "check":
            require(data.get("model") in ledger.plan["models"], "selection", "budget")
        elif args.action == "reserve":
            print(ledger.reserve(data["model"], data["scenario"], data["tokens"]))
        elif args.action == "register":
            ledger.register(data["cases"])
        elif args.action == "dispatched":
            ledger.dispatched(data["attempt"])
        else:
            ledger.finish(data["attempt"], data["state"], data["metrics"])
        return 0
    if args.command == "report":
        from probe_support.runtime import summary

        print(json.dumps(summary(ledger), indent=2))
        return 0
    models = args.model or ledger.plan["models"]
    require(
        all(model in ledger.plan["models"] for model in models), "selection", "setup"
    )
    cases = tuple((args.cases or ("image_generate" if ledger.is_images else "text,tool")).split(","))
    if args.dry_run:
        if ledger.is_images:
            from probe_support.image_generation import plan_groups
        else:
            from probe_support.scenarios import plan_groups

        groups = plan_groups(
            ledger,
            models,
            cases=cases,
            protocol=args.protocol,
            delivery=args.delivery,
            effort=args.effort,
        )
        print(
            json.dumps(
                {
                    "groups": [
                        {"scenario": group[4], "requests": group[5], "tokens": group[6],
                         **({"images": group[5] * ledger.plan["images_per_request"]} if ledger.is_images else {})}
                        for group in groups
                    ],
                    "remaining": ledger.plan["limit"]
                    - sum(row["state"] != "not_run" for row in ledger.snapshot()),
                }
            )
        )
        return 0
    require(args.live, "live_not_enabled", "setup")
    os.environ["MORPHIECORE_PROBE_LIVE"] = "1"
    if ledger.is_images:
        from probe_support.image_generation import matrix
    else:
        from probe_support.scenarios import matrix

    return (
        0
        if matrix(
            ledger,
            models,
            cases=cases,
            protocol=args.protocol,
            delivery=args.delivery,
            effort=args.effort,
        )
        else 1
    )


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception:
        print(
            "Probe stopped; private diagnostics suppressed. Inspect the bounded run ledger.",
            file=sys.stderr,
        )
        raise SystemExit(1)
