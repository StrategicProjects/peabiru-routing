#!/usr/bin/env python3
"""Check the local links of a built site: every href/src that points inside
the site must resolve to an existing file. External links (http, mailto) are
not fetched. Exits non-zero, listing the broken links, if any.

    python3 site/check_links.py _site
"""

import re
import sys
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import unquote, urlsplit


class Links(HTMLParser):
    def __init__(self):
        super().__init__()
        self.links = []

    def handle_starttag(self, tag, attrs):
        for k, v in attrs:
            if v and ((k == "href" and tag in ("a", "link")) or (k == "src" and tag in ("img", "script", "source"))):
                self.links.append(v)


def main(root):
    root = Path(root).resolve()
    broken = []
    # a 404 page is served from any path, so its links are absolute by design
    pages = [p for p in root.rglob("*.html") if p.name != "404.html"]
    # rustdoc pages are many and generated; check only the crate's index ones
    pages = [p for p in pages if "/rust/api/" not in str(p) or p.name == "index.html"]
    for page in pages:
        parser = Links()
        try:
            parser.feed(page.read_text(errors="replace"))
        except Exception as e:  # noqa: BLE001
            broken.append((page, f"unparsable: {e}"))
            continue
        for link in parser.links:
            u = urlsplit(link)
            if u.scheme or link.startswith("//") or link.startswith("#") or link.startswith("data:"):
                continue
            if re.match(r"^(javascript|mailto):", link):
                continue
            path = unquote(u.path)
            if not path:
                continue
            target = (root / path.lstrip("/")) if path.startswith("/") else (page.parent / path)
            target = target.resolve()
            if target.is_dir():
                target = target / "index.html"
            if not target.exists():
                broken.append((page.relative_to(root), link))
    for page, link in broken:
        print(f"broken: {page}: {link}")
    print(f"checked {len(pages)} pages, {len(broken)} broken local links")
    return 1 if broken else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1] if len(sys.argv) > 1 else "_site"))
