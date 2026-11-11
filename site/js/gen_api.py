#!/usr/bin/env python3
"""Generate the API reference of the JavaScript guide, site/js/src/api.md,
from the type declarations of the npm package, js/lib/index.d.ts (its
declarations and their /** ... */ comments), so that the reference cannot
drift from the types.

    python3 site/js/gen_api.py           write site/js/src/api.md
    python3 site/js/gen_api.py --check   exit 1 if api.md is not up to date

It also checks that the values the package exports (js/lib/index.js) are
the values index.d.ts declares, and fails if they differ.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DTS = ROOT / "js/lib/index.d.ts"
JS = ROOT / "js/lib/index.js"
OUT = ROOT / "site/js/src/api.md"

# Sections of the reference, in order; a declaration not listed here goes
# to "Other declarations", so a new one is never left out.
SECTIONS = [
    ("Networks", ["buildNetwork", "BuildNetworkInput", "GtfsFeed", "Bytes", "WarningHandler", "Network", "NetworkInfo"]),
    ("Travel-time matrices", ["travelTimeMatrix", "TravelTimeMatrixOptions", "TravelTimeMatrix"]),
    ("Accessibility", ["accessibility", "AccessibilityOptions", "AccessibilityTable"]),
    ("Expanded and arrival matrices", [
        "expandedTravelTimeMatrix", "arrivalTravelTimeMatrix", "ExpandedTravelTimeMatrixOptions",
        "ArrivalTravelTimeMatrixOptions", "ExpandedMatrix",
    ]),
    ("Points, dates and shared options", ["Points", "Numbers", "DateTime", "RoutingOptions"]),
    ("Missing values and errors", ["NA_INT32", "UnsupportedError"]),
    ("Loading the module and versions", ["init", "initSync", "InitInput", "coreVersion", "version"]),
]


def doc_text(lines):
    """The text of a /** ... */ comment, as Markdown."""
    text = []
    for line in lines:
        line = line.strip()
        line = re.sub(r"^/\*\*\s?", "", line)
        line = re.sub(r"\s?\*/$", "", line)
        line = re.sub(r"^\*\s?", "", line)
        text.append(line)
    out, para = [], []
    for line in text + [""]:
        if line:
            para.append(line)
        elif para:
            out.append(" ".join(para))
            para = []
    return "\n\n".join(out)


def depth_delta(s):
    return sum(s.count(c) for c in "({[<") - sum(s.count(c) for c in ")}]>") + s.count("=>")


def statement(lines, i, ends=(";",)):
    """Index of the last line of the declaration that starts at line i: the
    first line that ends with one of `ends` at depth 0."""
    depth = 0
    j = i
    while True:
        depth += depth_delta(lines[j])
        end = lines[j].rstrip()
        if depth <= 0 and end.endswith(ends):
            return j
        j += 1


def parse_members(body):
    """Members of an interface or class body: (doc, declaration) pairs."""
    members, doc, i = [], [], 0
    while i < len(body):
        s = body[i].strip()
        if not s:
            i += 1
        elif s.startswith("/**"):
            j = i
            while "*/" not in body[j]:
                j += 1
            doc = body[i:j + 1]
            i = j + 1
        else:
            j = statement(body, i)
            decl = " ".join(x.strip() for x in body[i:j + 1])
            members.append((doc_text(doc), decl.rstrip(";")))
            doc = []
            i = j + 1
    return members


def parse(src):
    lines = src.splitlines()
    decls, doc, i = [], [], 0
    while i < len(lines):
        s = lines[i]
        if s.startswith("/**"):
            j = i
            while "*/" not in lines[j]:
                j += 1
            doc = lines[i:j + 1]
            i = j + 1
            continue
        m = re.match(r"export (?:declare )?(const|function|class|type|interface) (\w+)", s)
        if not m:
            if s.strip() and not s.startswith("//"):
                sys.exit(f"gen_api.py: cannot read line {i + 1} of {DTS}: {s}")
            doc = [] if s.strip() else doc
            i += 1
            continue
        kind, name = m.groups()
        j = statement(lines, i, ("}",) if kind in ("interface", "class") else (";",))
        block = lines[i:j + 1]
        d = {"kind": kind, "name": name, "doc": doc_text(doc), "line": i + 1}
        if kind in ("interface", "class"):
            d["head"] = block[0].rstrip(" {")
            d["members"] = parse_members(block[1:-1])
            ext = re.search(r"\bextends (\w+)", block[0])
            d["extends"] = ext.group(1) if ext else None
        else:
            d["code"] = "\n".join(block)
        decls.append(d)
        doc = []
        i = j + 1
    return decls


def anchor(name):
    # the name itself, case and all: travelTimeMatrix (the function) and
    # TravelTimeMatrix (its result) need distinct ids
    return name


def code(text):
    """A Markdown code span, also for text that holds backticks."""
    return f"`` {text} ``" if "`" in text else f"`{text}`"


def link_types(text, names):
    """Turn `Name` code spans that name a declaration into links."""
    return re.sub(r"`(\w+)`", lambda m: f"[`{m.group(1)}`](#{anchor(m.group(1))})" if m.group(1) in names else m.group(0), text)


def render(decls):
    names = {d["name"] for d in decls}
    by_name = {d["name"]: d for d in decls}
    out = [
        "# API reference",
        "",
        "<!-- Generated by site/js/gen_api.py from js/lib/index.d.ts; do not edit by hand. -->",
        "",
        "Everything the package exports, from its type declarations",
        "(`lib/index.d.ts`, generated into this page when the site is built). Every",
        "function returns a Promise; options are the fields of an options object.",
        "",
    ]
    listed = [n for _, ns in SECTIONS for n in ns]
    sections = [(t, [n for n in ns if n in by_name]) for t, ns in SECTIONS]
    other = [d["name"] for d in decls if d["name"] not in listed]
    if other:
        sections.append(("Other declarations", other))
    out.append("| Section | Declarations |")
    out.append("|---|---|")
    for title, ns in sections:
        out.append(f"| {title} | " + ", ".join(f"[`{n}`](#{anchor(n)})" for n in ns) + " |")
    out.append("")
    for title, ns in sections:
        out += [f"## {title}", ""]
        for n in ns:
            d = by_name[n]
            kind = {"const": "constant", "function": "function", "class": "class", "type": "type", "interface": "interface"}[d["kind"]]
            out += [f"### {n} {{#{anchor(n)}}}", "", f"*{kind}*", ""]
            if "code" in d:
                out += ["```typescript", d["code"], "```", ""]
            else:
                out += ["```typescript", d["head"], "```", ""]
            if d["doc"]:
                out += [link_types(d["doc"], names - {n}), ""]
            if d["kind"] == "interface" and d.get("extends") in names:
                e = d["extends"]
                out += [f"Also takes every option of [`{e}`](#{anchor(e)}).", ""]
            for mdoc, decl in d.get("members", []):
                if decl.startswith("private "):
                    continue
                item = f"- {code(decl)}"
                if mdoc:
                    item += ": " + link_types(mdoc, names)
                out.append(item)
            if d.get("members"):
                out.append("")
    return "\n".join(out).rstrip() + "\n"


def check_exports(decls):
    values = {d["name"] for d in decls if d["kind"] in ("const", "function", "class")}
    js = set(re.findall(r"^export (?:async )?(?:const|let|function|class) (\w+)", JS.read_text(), re.M))
    if values != js:
        sys.exit(
            "gen_api.py: js/lib/index.js and js/lib/index.d.ts disagree:\n"
            f"  exported but not declared: {sorted(js - values)}\n"
            f"  declared but not exported: {sorted(values - js)}"
        )


def main():
    decls = parse(DTS.read_text())
    check_exports(decls)
    text = render(decls)
    if "--check" in sys.argv[1:]:
        if not OUT.exists() or OUT.read_text() != text:
            sys.exit(f"{OUT.relative_to(ROOT)} is out of date: run python3 site/js/gen_api.py")
        print(f"{OUT.relative_to(ROOT)} is up to date ({len(decls)} declarations)")
        return
    if not OUT.exists() or OUT.read_text() != text:
        OUT.write_text(text)
        print(f"wrote {OUT.relative_to(ROOT)} ({len(decls)} declarations)")


if __name__ == "__main__":
    main()
