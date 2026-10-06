"""One immutable plan and transactional attempt budget across processes.

Only closed metadata is durable. Reservations survive crashes and are never
refunded/replayed automatically. Corrupt or changed plans fail closed.
"""

import contextlib
import copy
import hashlib
import json
import os
from pathlib import Path
import re
import sqlite3
import time
import uuid
from .catalog import BINDINGS, IMAGE_BINDINGS, select_bindings, select_image_bindings
from .checks import require

MODELS = {row[1]: row for row in (*BINDINGS, *IMAGE_BINDINGS)}


def source_fingerprint(root=None):
    root = Path(__file__).resolve().parents[2] if root is None else root
    paths = [root / name for name in ("Cargo.toml", "Cargo.lock", "tests/sdk/uv.lock",
             "tests/sdk/pyproject.toml", "tests/sdk/.python-version", "rust-toolchain.toml",
             "package.json", "package-lock.json", "tsconfig.json")]
    for pattern in (
        "src/**/*.rs",
        "examples/**/*.rs",
        "examples/**/*.py",
        "examples/**/*.ts",
    ):
        paths.extend(root.glob(pattern))
    digest = hashlib.sha256()
    for path in sorted(set(paths)):
        digest.update(str(path.relative_to(root)).encode())
        digest.update(b"\0")
        digest.update(path.read_bytes())
    return digest.hexdigest()


NUMBERS = {
    "http",
    "image_count", "image_bytes", "image_width", "image_height",
    "upstream_status",
    "retry_after_seconds",
    "elapsed_ms",
    "wire_bytes",
    "text_chars",
    "reasoning_chars",
    "tool_calls",
    "reported_output_tokens",
    "reported_input_tokens",
    "reported_reasoning_tokens",
    "reported_image_tokens",
    "reported_cached_tokens",
    "reported_logprob_slots",
    "reported_opaque_items",
    "handed_off_bytes",
    "received_bytes",
    "upstream_head_ms",
    "first_upstream_bytes_ms",
    "event_items", "event_reasoning_items", "event_parts", "event_deltas", "event_item_closures",
}
BOOLS = {
    "sdk_consumed",
    "image_decoded", "image_usage_omitted", "image_billing_omitted",
    "wire_closed",
    "content_ok",
    "history_ok",
    "client_closed_before_terminal",
    "exact_answer",
}
ENUMS = {
    "decode_failure": {"invalid_sequence", "invalid_metadata", "invalid_item_snapshot",
        "invalid_terminal_snapshot", "invalid_value_snapshot", "invalid_reasoning",
        "invalid_probabilities", "invalid_item", "invalid_identity", "invalid_other",
        "unsupported", "event_identity", "event_lifecycle", "limit", "missing_terminal",
        "semantic", "framing", "other", "snapshot_shape", "snapshot_identity",
        "snapshot_lifecycle", "snapshot_phase", "snapshot_replay", "snapshot_summary",
        "snapshot_replay_added", "snapshot_replay_removed", "snapshot_replay_changed",
        "snapshot_text", "snapshot_annotations", "snapshot_probability_presence", "snapshot_probabilities"},
    "oracle_failure": {
        "exact_text", "image_format", "image_decode", "image_pixels", "visual_math_format", "visual_math_value",
        "visual_math_calls", "file_marker", "file_math", "missing_opaque", "unexpected_terminal", "other",
    },
    "operator_outcome": {"error", "interrupted", "timeout", "shutdown", "complete"},
    "stage": {
        "admission",
        "prepare",
        "connect",
        "response_head",
        "intake",
        "terminal",
        "projection",
        "delivery",
        "complete",
        "wire",
        "oracle",
        "unknown",
    },
    "terminal": {
        "stop",
        "image.complete",
        "tool_calls",
        "length",
        "content_filter",
        "response.completed",
        "response.incomplete",
        "response.failed",
        "error",
        "missing",
    },
    "failure": {
        "http",
        "transport",
        "consumer",
        "wire",
        "oracle",
        "budget",
        "setup",
        "unknown",
    },
}


def closed_metrics(metrics):
    result = {}
    for key, value in metrics.items():
        if key == "reported_image_cost_usd":
            require(isinstance(value, str) and len(value) <= 128 and re.fullmatch(r"[0-9]+(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?", value) is not None, "report_cost", "setup")
        elif key in NUMBERS:
            require(
                value is None or type(value) is int and 0 <= value <= 10**12,
                "report_number",
                "setup",
            )
        elif key in BOOLS:
            require(type(value) is bool, "report_bool", "setup")
        elif key in ENUMS:
            require(value in ENUMS[key] or key == "decode_failure" and value is None, "report_enum", "setup")
        else:
            raise RuntimeError("unknown diagnostic field")
        result[key] = value
    return result


class Run:
    @classmethod
    def create(
        cls,
        directory,
        *,
        providers="nvidia",
        models=None,
        limit=32,
        tokens=2048,
        continue_oracle=False,
        task="generation",
        images_per_request=None,
    ):
        require(task in ("generation", "images"), "plan_task", "setup")
        image_task = task == "images"
        require(images_per_request is None or image_task, "plan_task", "setup")
        image_count = 1 if images_per_request is None else images_per_request
        require(type(image_count) is int and 1 <= image_count <= 10, "plan_images", "setup")
        rows = select_image_bindings(providers, models) if image_task else select_bindings(providers, models=models)
        require(
            type(limit) is int
            and 1 <= limit <= 256
            and (tokens is None if image_task else type(tokens) is int and 1 <= tokens <= 2048),
            "plan_budget",
            "setup",
        )
        directory = Path(directory)
        directory.mkdir(mode=0o700, parents=True, exist_ok=False)
        plan = {
            "version": 2 if image_task else 1,
            "id": uuid.uuid4().hex,
            "models": [row[1] for row in rows],
            "limit": limit,
            "tokens": tokens,
            "continue_oracle": bool(continue_oracle),
            "created": int(time.time()),
            "expires": int(time.time()) + 86400,
            "source_fingerprint": source_fingerprint(),
            "sdk": "3.24.0",
            "pi": "1.0.4",
        }
        if image_task:
            plan["images_per_request"] = image_count
        raw = json.dumps(plan, sort_keys=True).encode()
        fd = os.open(
            directory / "plan.json", os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600
        )
        with os.fdopen(fd, "wb") as file:
            file.write(raw)
        db = sqlite3.connect(directory / "ledger.sqlite3")
        os.chmod(directory / "ledger.sqlite3", 0o600)
        with db:
            db.execute("CREATE TABLE identity (digest TEXT NOT NULL)")
            db.execute(
                "INSERT INTO identity VALUES (?)", (hashlib.sha256(raw).hexdigest(),)
            )
            db.execute(
                "CREATE TABLE attempts (id INTEGER PRIMARY KEY, scenario TEXT UNIQUE NOT NULL, model TEXT NOT NULL, tokens INTEGER, state TEXT NOT NULL, metrics TEXT NOT NULL, source TEXT NOT NULL)"
            )
            db.execute("CREATE TABLE blocked (model TEXT PRIMARY KEY)")
            db.execute(
                "CREATE TABLE cases (scenario TEXT PRIMARY KEY, model TEXT NOT NULL, tokens INTEGER)"
            )
        db.close()
        return cls(directory)

    def __init__(self, directory):
        self.directory = Path(directory).resolve()
        raw = (self.directory / "plan.json").read_bytes()
        require(len(raw) <= 16384, "plan_size", "setup")
        self._plan = json.loads(raw)
        require(
            set(self.plan)
            == {
                "version",
                "id",
                "models",
                "limit",
                "tokens",
                "continue_oracle",
                "created",
                "expires",
                "source_fingerprint",
                "sdk",
                "pi",
            } | ({"images_per_request"} if self._plan.get("version") == 2 else set()),
            "plan_shape",
            "setup",
        )
        require(
            type(self.plan["version"]) is int and self.plan["version"] in (1, 2)
            and re.fullmatch("[0-9a-f]{32}", self.plan["id"]) is not None,
            "plan_id",
            "setup",
        )
        require(
            type(self.plan["limit"]) is int and 1 <= self.plan["limit"] <= 256,
            "plan_limit",
            "setup",
        )
        require(
            (self.plan["tokens"] is None and type(self.plan["images_per_request"]) is int and 1 <= self.plan["images_per_request"] <= 10
             if self.is_images else type(self.plan["tokens"]) is int and 1 <= self.plan["tokens"] <= 2048),
            "plan_tokens",
            "setup",
        )
        require(
            self.plan["models"]
            and all(model in MODELS and (MODELS[model][4] == ("images",)) == self.is_images for model in self.plan["models"]),
            "plan_model",
            "setup",
        )
        self.digest = hashlib.sha256(raw).hexdigest()
        self.source = source_fingerprint()
        with self._db() as db:
            self._verify(db)

    @property
    def is_images(self):
        return self._plan["version"] == 2

    def valid_budget(self, model, tokens):
        return model in self.plan["models"] and (
            tokens is None if self.is_images else
            type(tokens) is int and 1 <= tokens <= self.plan["tokens"]
        )

    @property
    def plan(self):
        return copy.deepcopy(self._plan)

    @contextlib.contextmanager
    def _db(self):
        # mode=rw must not manufacture a replacement for a missing ledger.
        db = sqlite3.connect(
            (self.directory / "ledger.sqlite3").as_uri() + "?mode=rw",
            uri=True,
            timeout=3,
        )
        try:
            db.execute("PRAGMA synchronous=FULL")
            db.execute("BEGIN IMMEDIATE")
            yield db
            db.commit()
        except BaseException:
            db.rollback()
            raise
        finally:
            db.close()

    def _verify(self, db, *, active=False):
        require(
            db.execute("SELECT digest FROM identity").fetchall() == [(self.digest,)],
            "plan_changed",
            "setup",
        )
        if active:
            require(time.time() <= self.plan["expires"], "plan_expired", "budget")
        # Also detect edits made after this object was opened.
        require(
            hashlib.sha256((self.directory / "plan.json").read_bytes()).hexdigest()
            == self.digest,
            "plan_changed",
            "setup",
        )

    def register(self, cases):
        require(len(cases) <= 256, "case_count", "setup")
        with self._db() as db:
            self._verify(db, active=True)
            for model, scenario, tokens in cases:
                require(
                    self.valid_budget(model, tokens),
                    "selection",
                    "budget",
                )
                require(
                    re.fullmatch("[a-zA-Z0-9_:.-]{1,128}", scenario) is not None,
                    "scenario",
                    "setup",
                )
                previous = db.execute(
                    "SELECT model,tokens FROM cases WHERE scenario=?", (scenario,)
                ).fetchone()
                require(
                    previous is None or previous == (model, tokens),
                    "case_changed",
                    "setup",
                )
                db.execute(
                    "INSERT OR IGNORE INTO cases VALUES (?,?,?)",
                    (scenario, model, tokens),
                )
            require(
                db.execute("SELECT count(*) FROM cases").fetchone()[0] <= 256,
                "case_count",
                "setup",
            )

    def reserve(self, model, scenario, tokens):
        require(
            source_fingerprint() == self.source,
            "source_changed_during_process",
            "setup",
        )
        require(
            self.valid_budget(model, tokens),
            "selection",
            "budget",
        )
        require(
            isinstance(scenario, str)
            and re.fullmatch("[a-zA-Z0-9_:.-]{1,128}", scenario) is not None,
            "scenario",
            "setup",
        )
        with self._db() as db:
            self._verify(db, active=True)
            require(
                not db.execute(
                    "SELECT 1 FROM blocked WHERE model=?", (model,)
                ).fetchone(),
                "provider_stopped",
                "budget",
            )
            require(
                db.execute("SELECT count(*) FROM attempts").fetchone()[0]
                < self.plan["limit"],
                "run_exhausted",
                "budget",
            )
            require(
                not db.execute(
                    "SELECT 1 FROM attempts WHERE scenario=?", (scenario,)
                ).fetchone(),
                "already_reserved",
                "budget",
            )
            row = db.execute(
                "INSERT INTO attempts(scenario,model,tokens,state,metrics,source) VALUES (?,?,?,?,?,?)",
                (scenario, model, tokens, "reserved", "{}", self.source),
            )
            return f"{self.plan['id']}:{row.lastrowid}"

    def _number(self, attempt):
        prefix, number = attempt.split(":", 1)
        require(prefix == self.plan["id"] and number.isdigit(), "attempt_id", "setup")
        return int(number)

    def dispatched(self, attempt):
        with self._db() as db:
            self._verify(db, active=True)
            row = db.execute(
                "UPDATE attempts SET state='dispatched' WHERE id=? AND state='reserved'",
                (self._number(attempt),),
            )
            require(row.rowcount == 1, "attempt_state", "setup")

    def finish(self, attempt, state, metrics):
        require(
            state in ("passed", "oracle_failed", "failed", "cancelled"),
            "result_state",
            "setup",
        )
        metrics = closed_metrics(metrics)
        with self._db() as db:
            self._verify(db)
            number = self._number(attempt)
            row = db.execute(
                "SELECT model,state FROM attempts WHERE id=?", (number,)
            ).fetchone()
            require(
                row is not None and row[1] in ("reserved", "dispatched"),
                "attempt_state",
                "setup",
            )
            db.execute(
                "UPDATE attempts SET state=?,metrics=? WHERE id=?",
                (state, json.dumps(metrics, sort_keys=True), number),
            )
            if (
                state == "failed"
                or state == "oracle_failed"
                and not self.plan["continue_oracle"]
            ):
                db.execute("INSERT OR IGNORE INTO blocked VALUES (?)", (row[0],))

    def snapshot(self):
        with self._db() as db:
            self._verify(db)
            rows = db.execute(
                "SELECT id,scenario,model,tokens,state,metrics,source FROM attempts ORDER BY id"
            ).fetchall()
            pending = db.execute(
                "SELECT scenario,model,tokens FROM cases WHERE scenario NOT IN (SELECT scenario FROM attempts) ORDER BY scenario"
            ).fetchall()
        return [
            {
                "attempt": f"{self.plan['id']}:{i}",
                "scenario": case,
                "model": model,
                "tokens": tokens,
                **({"images": self.plan["images_per_request"]} if self.is_images else {}),
                "state": state,
                "metrics": json.loads(metrics),
                "source_fingerprint": source,
            }
            for i, case, model, tokens, state, metrics, source in rows
        ] + [
            {
                "attempt": None,
                "scenario": case,
                "model": model,
                "tokens": tokens,
                **({"images": self.plan["images_per_request"]} if self.is_images else {}),
                "state": "not_run",
                "metrics": {},
            }
            for case, model, tokens in pending
        ]
