"""Verify vendored upstream bytes offline; --fetch restores missing/corrupt raw assets.

No package installation, upstream code execution, floating refs or implicit network.
Literal framing extracts are regenerated from verified TypeScript text, not executed.
"""
import argparse
import ast
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import tempfile
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
LIMIT = 2 << 20


def digest(data):
    return hashlib.sha256(data).hexdigest()


def relative(value):
    path = PurePosixPath(value)
    if not value or path.is_absolute() or any(x in ("", ".", "..") for x in value.split("/")):
        raise ValueError("invalid asset path")
    return path


def local_path(root, name):
    path = root / relative(name)
    if any(parent.is_symlink() for parent in (path, *path.parents)):
        raise ValueError("asset symlink")
    return path


def literal_chunks(source, function):
    """Only the literal onChunk calls in the three reviewed framing functions."""
    if function not in {
        "getMultilineFixtureStream", "getCarriageReturnFixtureStream",
        "getCarriageReturnLineFeedFixtureStream",
    }:
        raise ValueError("unreviewed framing function")
    marker = f"export async function {function}("
    if source.count(marker) != 1:
        raise ValueError("missing or ambiguous framing function")
    body = source.split(marker, 1)[1].split("\nexport async function ", 1)[0]
    values = []
    for line in body.splitlines():
        line = line.strip()
        if not line.startswith("onChunk("):
            continue
        if line == "onChunk(encode({event: 'done', data: '✔'}))":
            continue  # The generated sentinel is not part of the literal cases.
        match = re.fullmatch(r"onChunk\(('(?:[^'\\]|\\.)*')\)", line)
        if not match:
            raise ValueError("unreviewed framing expression")
        values.append(ast.literal_eval(match[1]))
    if not values:
        raise ValueError("empty framing extraction")
    return "".join(values).encode()


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise ValueError("asset redirect refused")


def download(source, entry):
    repo = source["repository"]
    if not re.fullmatch(r"https://github\.com/[\w.-]+/[\w.-]+", repo):
        raise ValueError("untrusted asset repository")
    path = relative(entry["path"])
    url = repo.replace("https://github.com/", "https://raw.githubusercontent.com/")
    url += f'/{entry["revision"]}/{path}'
    opener = urllib.request.build_opener(NoRedirect())
    with opener.open(urllib.request.Request(url, headers={
        "User-Agent": "MorphieCore-pinned-test-assets",
    }), timeout=30) as response:
        data = response.read(LIMIT + 1)
    if len(data) > LIMIT or digest(data) != entry["sha256"]:
        raise ValueError("download size/digest mismatch")
    return data


def replace(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as output:
        temporary = Path(output.name)
        try:
            output.write(data)
            output.flush()
            temporary.replace(path)
        finally:
            temporary.unlink(missing_ok=True)


def verify(root=ROOT, fetch=False):
    lock = json.loads((root / "tests/upstream/sources.lock.json").read_text())
    assets = json.loads((root / "tests/upstream/assets.lock.json").read_text())
    if assets["format"] != 1:
        raise ValueError("unknown asset format")
    base = root / "testdata/upstream"
    names = [x["local"] for x in assets["files"] + assets["framing"]]
    if len(names) != len(set(names)):
        raise ValueError("duplicate asset destination")
    for entry in assets["files"]:
        source = lock["sources"][entry["source"]]
        if (entry["revision"] != source["revision"]
                or not re.fullmatch(r"[0-9a-f]{40}", entry["revision"])
                or not re.fullmatch(r"[0-9a-f]{64}", entry["sha256"])):
            raise ValueError("asset/source pin mismatch")
        path = local_path(base, entry["local"])
        if not path.exists() or digest(path.read_bytes()) != entry["sha256"]:
            if not fetch:
                raise ValueError(f'missing/changed asset: {entry["local"]}')
            replace(path, download(source, entry))
    for entry in assets["framing"]:
        source = local_path(base, entry["from"])
        if entry["from"] not in [x["local"] for x in assets["files"]]:
            raise ValueError("unverified extraction source")
        data = literal_chunks(source.read_text(), entry["function"])
        if digest(data) != entry["sha256"]:
            raise ValueError("framing extraction changed")
        path = local_path(base, entry["local"])
        if fetch:
            replace(path, data)
        if path.read_bytes() != data:
            raise ValueError(f'changed extraction: {entry["local"]}')
    return len(names)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fetch", action="store_true")
    args = parser.parse_args()
    print(f"Verified {verify(fetch=args.fetch)} pinned upstream assets")
