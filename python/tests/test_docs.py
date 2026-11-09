"""Every code example of the documentation runs, and prints what the page
shows.

In each page of ``docs/``, the ``python`` code blocks run in order in one
namespace; a ``text`` block that follows a code block (with only blank
lines between) is its output and must match what the code prints. Run with
``PEABIRU_UPDATE_DOCS=1`` to rewrite those output blocks with the real
output instead of comparing.

The docstring examples (numpydoc ``Examples`` sections, which feed the API
reference) run as doctests.
"""

from __future__ import annotations

import contextlib
import doctest
import io
import os
import re
from pathlib import Path

import pytest

import peabiru
import peabiru._data
import peabiru._network
import peabiru._routing

DOCS = Path(__file__).resolve().parents[1] / "docs"
PAGES = sorted(p for p in DOCS.glob("*.md"))
FENCE = re.compile(r"^```(\w*)[^\n]*\n(.*?)^```[ \t]*$", re.S | re.M)


def blocks(text: str):
    """(lang, body, start, end) of every fenced block."""
    return [(m.group(1), m.group(2), m.start(2), m.end(2)) for m in FENCE.finditer(text)]


@pytest.mark.parametrize("page", PAGES, ids=[p.name for p in PAGES])
def test_page_examples(page: Path, tmp_path, monkeypatch):
    monkeypatch.chdir(tmp_path)
    text = page.read_text(encoding="utf-8")
    bl = blocks(text)
    ns: dict = {"__name__": "__main__"}
    replacements = []
    for k, (lang, body, _, _) in enumerate(bl):
        if lang != "python":
            continue
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            exec(compile(body, f"{page.name}[block {k}]", "exec"), ns)
        nxt = bl[k + 1] if k + 1 < len(bl) else None
        if nxt is None or nxt[0] != "text":
            continue
        between = text[FENCE_END(text, bl[k]) : FENCE_START(text, nxt)]
        if between.strip():
            continue
        if os.environ.get("PEABIRU_UPDATE_DOCS") == "1":
            replacements.append((nxt[2], nxt[3], out.getvalue()))
        else:
            assert out.getvalue().rstrip() == nxt[1].rstrip(), f"{page.name}, block {k}"
    if replacements:
        for start, end, new in sorted(replacements, reverse=True):
            text = text[:start] + new.rstrip() + "\n" + text[end:]
        page.write_bytes(text.encode("utf-8"))


def FENCE_END(text: str, block) -> int:
    """Index right after the closing fence of a block."""
    return text.index("```", block[3]) + 3


def FENCE_START(text: str, block) -> int:
    """Index of the opening fence of a block."""
    return text.rindex("```", 0, block[2] - 1)


@pytest.mark.parametrize(
    "module",
    [peabiru, peabiru._data, peabiru._network, peabiru._routing],
    ids=lambda m: m.__name__,
)
def test_docstring_examples(module):
    res = doctest.testmod(
        module, optionflags=doctest.ELLIPSIS | doctest.NORMALIZE_WHITESPACE, verbose=False
    )
    assert res.failed == 0
