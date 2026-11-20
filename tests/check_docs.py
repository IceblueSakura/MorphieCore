"""Check tracked documentation with GitHub-flavored Markdown rendering, offline."""

from html.parser import HTMLParser
from pathlib import Path
import posixpath
import subprocess
from urllib.parse import unquote, urlsplit


class Page(HTMLParser):
    def __init__(self, markdown):
        super().__init__()
        self.anchors = set()
        self.links = []
        self.sections = []
        self.feed(subprocess.check_output(
            ["pandoc", "--sandbox", "-f", "gfm", "-t", "html5"],
            input=markdown, text=True,
        ))
        self.close()

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        anchor = attrs.get("id")
        if anchor:
            self.anchors.add(anchor)
        if tag in ("h1", "h2", "h3", "h4", "h5", "h6"):
            level = int(tag[1])
            while self.sections and self.sections[-1][0] >= level:
                self.sections.pop()
            self.sections.append((level, anchor))
        attr = {"a": "href", "img": "src"}.get(tag)
        if attrs.get(attr) is not None:
            self.links.append((attrs[attr], {name for _, name in self.sections}))


def check(root):
    tracked = {Path(path) for path in subprocess.check_output(
        ["git", "ls-files", "-z"], cwd=root, text=True,
    ).split("\0") if path}
    destinations = tracked | {parent for path in tracked for parent in path.parents}
    pages = {
        path: Page((root / path).read_text(encoding="utf-8"))
        for path in sorted(tracked) if path.suffix == ".md"
    }
    required = {
        (Path("docs/usage/gateway-configuration.md"), "configuration-templates"): {
            path for path in tracked
            if path.parent == Path("config") and ".example." in path.name
        },
        (Path("docs/references.md"), "research-notes"): {
            path for path in pages if path.is_relative_to("docs/refs")
        },
    }
    errors = []
    link_count = 0
    for source, page in pages.items():
        for link, sections in page.links:
            try:
                url = urlsplit(link)
            except ValueError:
                errors.append(f"{source}: invalid link {link!r}")
                continue
            if url.scheme or url.netloc:
                continue
            target = unquote(url.path)
            if target.startswith("/"):
                destination = Path(posixpath.normpath(target.lstrip("/")))
            elif target:
                destination = Path(posixpath.normpath(source.parent / target))
            else:
                destination = source
            link_count += 1
            if destination not in destinations or not (root / destination).exists():
                errors.append(f"{source}: missing tracked path {link!r}")
                continue
            if (url.fragment and destination in pages
                    and unquote(url.fragment) not in pages[destination].anchors):
                errors.append(f"{source}: missing anchor {link!r}")
                continue
            for section in sections:
                if (source, section) in required:
                    required[source, section].discard(destination)

    for (source, section), missing in required.items():
        for path in sorted(missing):
            errors.append(f"{source}#{section}: missing index entry for {path}")
    if errors:
        raise SystemExit("\n".join(errors))
    print(f"Checked {link_count} local links in {len(pages)} Markdown files, "
          "public configuration templates, and the research index")


if __name__ == "__main__":
    check(Path(__file__).resolve().parents[1])
