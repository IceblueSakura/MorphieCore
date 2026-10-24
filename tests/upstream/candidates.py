"""Explicit, read-only latest-stable check; never update pins, assets or packages.

Serial public metadata requests, no credentials/retries/redirects, 15-second socket
timeout and 2 MiB per response. Offline regression injects local responses.
"""
import argparse
import json
import re
from urllib.parse import quote
import urllib.request

from assets import ROOT, LIMIT, NoRedirect

REGISTRIES = {
    "openai-python": ("pypi", "openai"),
    "anthropic-python": ("pypi", "anthropic"),
    "google-python": ("pypi", "google-genai"),
    "openai-node": ("npm", "openai"),
    "anthropic-typescript": ("npm", "@anthropic-ai/sdk"),
    "google-js": ("npm", "@google/genai"),
    "eventsource-parser": ("npm", "eventsource-parser"),
}


def metadata(url):
    if not re.match(r"https://(api\.github\.com|pypi\.org|registry\.npmjs\.org)/", url):
        raise ValueError("untrusted metadata URL")
    opener = urllib.request.build_opener(NoRedirect())
    request = urllib.request.Request(url, headers={"User-Agent": "MorphieCore-source-candidates"})
    with opener.open(request, timeout=15) as response:
        data = response.read(LIMIT + 1)
    if len(data) > LIMIT:
        raise ValueError("metadata size limit")
    return json.loads(data)


def candidate(name, source, fetch=metadata):
    if source["selection"] == "source-snapshot-no-release":
        return {"source": name, "status": "snapshot-not-latest-release"}
    if source["selection"] != "stable-release":
        raise ValueError("unknown source selection")
    repo = source["repository"]
    if not re.fullmatch(r"https://github\.com/[\w.-]+/[\w.-]+", repo):
        raise ValueError("untrusted repository")
    api = repo.replace("https://github.com/", "https://api.github.com/repos/")
    registry, package = REGISTRIES.get(name, ("npm", source.get("package")))
    if package:
        if registry == "pypi":
            version = fetch(f"https://pypi.org/pypi/{quote(package, safe='')}/json")["info"]["version"]
        else:
            version = fetch(f"https://registry.npmjs.org/{quote(package, safe='')}/latest")["version"]
        if not re.fullmatch(r"\d+\.\d+\.\d+", version):
            raise ValueError("non-stable registry candidate")
        tag = re.sub(r"\d+\.\d+\.\d+.*$", version, source["release"])
    else:
        release = fetch(api + "/releases/latest")
        if release["prerelease"] or release["draft"]:
            raise ValueError("non-stable release candidate")
        tag = release["tag_name"]
    obj = fetch(api + "/git/ref/tags/" + quote(tag, safe=""))["object"]
    # At most one annotated tag is followed. Nested or malformed refs fail visibly.
    if obj["type"] == "tag":
        if not re.fullmatch(r"[0-9a-f]{40}", obj["sha"]):
            raise ValueError("invalid tag object")
        obj = fetch(api + "/git/tags/" + obj["sha"])["object"]
    if obj["type"] != "commit" or not re.fullmatch(r"[0-9a-f]{40}", obj["sha"]):
        raise ValueError("unresolved release commit")
    same = tag == source["release"] and obj["sha"] == source["revision"]
    return {"source": name, "pinned_release": source["release"], "candidate_release": tag,
            "candidate_revision": obj["sha"], "status": "unchanged" if same else "review-required"}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check-latest", action="store_true", required=True)
    parser.parse_args()
    sources = json.loads((ROOT / "tests/upstream/sources.lock.json").read_text())["sources"]
    failed = False
    for name, source in sources.items():
        try:
            result = candidate(name, source)
            failed |= result["status"] == "review-required"
        except Exception as error:
            result = {"source": name, "status": "check-failed", "error": str(error)}
            failed = True
        print(json.dumps(result), flush=True)
    raise SystemExit(1 if failed else 0)
