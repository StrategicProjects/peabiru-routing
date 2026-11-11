"""Every code example of the documentation runs, and prints what the page
shows.

In each page of ``docs/``, the ``python`` code blocks run in order in one
namespace; a ``text`` block that follows a code block (with only blank
lines between) is its output and must match what the code prints. Run with
``PEABIRU_UPDATE_DOCS=1`` to rewrite those output blocks with the real
output instead of comparing.

A page can declare what its examples need beyond Python packages with a
comment ``<!-- docs-test: needs osmium, env PEABIRU_EXAMPLE_DATA -->``: the
commands must be on the ``PATH`` and the environment variables set, or the
page is skipped. The Recife and Caruaru example uses it: it downloads about
640 MB into ``PEABIRU_EXAMPLE_DATA`` and cuts them with osmium, so it runs
only where both are there on purpose.

The docstring examples (numpydoc ``Examples`` sections, which feed the API
reference) run as doctests.
"""

from __future__ import annotations

import ast
import contextlib
import doctest
import importlib.util
import io
import os
import re
import shutil
from pathlib import Path

import pytest

import peabiru
import peabiru._data
import peabiru._network
import peabiru._network_tools
import peabiru._routing

DOCS = Path(__file__).resolve().parents[1] / "docs"
PAGES = sorted(p for p in DOCS.glob("*.md"))
NEEDS = re.compile(r"<!--\s*docs-test:\s*needs\s+(.*?)\s*-->")
FENCE = re.compile(r"^```(\w*)[^\n]*\n(.*?)^```[ \t]*$", re.S | re.M)


def blocks(text: str):
    """(lang, body, start, end) of every fenced block."""
    return [(m.group(1), m.group(2), m.start(2), m.end(2)) for m in FENCE.finditer(text)]


def imported_modules(code: str) -> set:
    """Top-level names of the modules a code block imports."""
    names = set()
    for node in ast.walk(ast.parse(code)):
        if isinstance(node, ast.Import):
            names.update(a.name.split(".")[0] for a in node.names)
        elif isinstance(node, ast.ImportFrom) and node.level == 0 and node.module:
            names.add(node.module.split(".")[0])
    return names


@pytest.mark.parametrize("page", PAGES, ids=[p.name for p in PAGES])
def test_page_examples(page: Path, tmp_path, monkeypatch):
    monkeypatch.chdir(tmp_path)
    text = page.read_text(encoding="utf-8")
    bl = blocks(text)
    # a page whose examples need an optional package (geopandas, from the
    # "geo" extra) runs only where it is installed
    needed = set().union(*(imported_modules(b) for lang, b, _, _ in bl if lang == "python"))
    missing = sorted(m for m in needed if importlib.util.find_spec(m) is None)
    if missing:
        pytest.skip(f"the examples need {', '.join(missing)}, not installed")
    for m in NEEDS.finditer(text):
        for need in (n.strip() for n in m.group(1).split(",")):
            if need.startswith("env "):
                if not os.environ.get(need[4:].strip()):
                    pytest.skip(f"the examples need {need[4:].strip()} set")
            elif shutil.which(need) is None:
                pytest.skip(f"the examples need the command {need}")
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
    [peabiru, peabiru._data, peabiru._network, peabiru._network_tools, peabiru._routing],
    ids=lambda m: m.__name__,
)
def test_docstring_examples(module):
    res = doctest.testmod(
        module, optionflags=doctest.ELLIPSIS | doctest.NORMALIZE_WHITESPACE, verbose=False
    )
    assert res.failed == 0
