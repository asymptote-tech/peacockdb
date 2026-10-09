#!/usr/bin/env python3
"""ticket_widget.py — the PR coverage widget as one HTML page, with readable ticket cells.

The PR comment `cost-report --md` writes lists each query's tickets as bare numbers
(`#152 #220`). This renders the same tables — every dataset the tree's cost-report covers —
as one standalone HTML page:

- a contents list at the top, one section per dataset rather than collapsible blocks;
- under each dataset's title, how many of its queries plan, run on the cpu and run on the
  device at all five modes (fully), at some of them (partially), and the total;
- in TPC-DS, the queries that plan at no mode move to the end of the table, under "Window
  functions" and "DataFusion upgrade" (a ticket in `df-upgrade.md`); any other stays in place;
- every mode glyph links to the query's section for that mode: the plan in `<mode>.plans.txt`,
  the cpu run in `<mode>-mini.cpu.txt`, the device answer in `gpu-result.txt` (a glyph standing
  for all five modes links the last one's);
- each query's two oracles before the plan columns and its schema validation after the gpu
  ones, from the registry's `cpu_oracle`, `gpu_oracle` and `schema_validation` columns, or
  from `corpus_cases.inc` on a tree whose registry has none;
- every query name links to its SQL, and every DuckDB Σout to its `<q>.duckdb_cost.txt`;
- a by-ticket table first: each cited ticket, its title, and per dataset the queries citing it, each
  linking to that query's row;
- every ticket reads `#152 (streamed probe build)`: the number links to the ticket's body in
  its markdown file on GitHub, and hovering it shows the full title.

The short description is the parenthesised part of a ticket header,
`### #152 (streamed probe build) GpuHashJoin: …`. A header without one — the archive's
`### #45 — …` — shows nothing; a ticket in the archive is struck through and marked `(closed)`.

    scripts/ticket_widget.py                          # this tree -> testdata/report.html
    scripts/ticket_widget.py --root ../other-checkout --wiki llm-wiki=master --wiki ../other-checkout/llm-wiki=ENS-x
    scripts/ticket_widget.py --md pr_comment.md       # skip cargo, render an existing comment

`testdata/report.html` is checked in, and a commit that changes `testdata/cost-registry.csv`
regenerates it. It prints no machine path, sha or time, so the same tree always writes the same
page; its links point at `--blob` and `--ref`, both `master` by default.

The tables come from cost-report itself, so the page shows what the PR shows; this script
rewrites cells and lays the page out. cost-report runs with the tree's own cargo target dir
unless CARGO_TARGET_DIR says otherwise — never point two worktrees at one target dir.
"""

import argparse
import html
import re
import subprocess
import sys
import tempfile
from pathlib import Path

DEFAULT_REPO = "asymptote-tech/peacockdb"
ANCHOR = re.compile(r'<a id="t(\d+)"></a>')
HEADER = re.compile(r"### #(\d+)\s*(?:\(([^)]*)\))?\s*(?:—\s*)?(.*)$")
SECTION = re.compile(
    r"<details><summary>(?P<label>.+?) — (?P<summary>[^<]*)</summary>\s*"
    r"(?P<legend><sub>.*?</sub>)\s*(?P<table><table>.*?</table>)\s*</details>", re.S)
CELL = re.compile(r"<td[^>]*>.*?</td>", re.S)
TICKETS = re.compile(r"#\d+(?: #\d+)*")
# The goldens directory and the SQL directory of each dataset label cost-report prints.
DATASETS = {
    "TPC-H": ("testdata/goldens/tpch.sf1", "testdata/tpch-queries"),
    "TPC-DS": ("testdata/goldens/tpcds.sf1", "testdata/tpcds-queries"),
    "pbench": ("testdata/goldens/pbench.sf1", "testdata/pbench-queries"),
}
REGISTRY_NAME = {"TPC-H": "tpch", "TPC-DS": "tpcds", "pbench": "pbench"}
# How each oracle compares, as `test_support/corpus.rs` (cpu) and `corpus_gpu.rs` (gpu) do it.
ORACLE_HOW = {
    "data_fusion_exact": "Same rows as plain DataFusion at target_partitions = 1, exactly, in any order (#287).",
    "data_fusion_approximate": "As exact, but Float64 columns may differ from DataFusion's by 1e-12 relative (multi-lane summation order).",
    "data_fusion_subset": "For an unordered LIMIT: the row count must match and the rows must be a sub-multiset of DataFusion's unlimited answer.",
    "golden_exact": "Same rows as the committed .result.txt section the cpu wrote, exactly, in any order (#287).",
    "golden_approx": "As golden exact, with a 1e-12 relative tolerance on float columns.",
    "golden_approx_std": "As golden exact, with a 1e-11 relative tolerance on float columns, for stddev and var.",
    "live_cpu": "Compared with a cpu run at the same mode, since no committed answer can serve (over the size cap, or rows the SQL does not determine).",
    "skip": "The device's answer is not compared.",
}


def declarations(root: Path) -> dict:
    """(dataset, query) -> (cpu oracle, gpu oracle, schema validation). From the registry's own
    columns where it has them; else from `corpus_cases.inc`, whose lines end in those three
    whatever comes before them (a tree that also declares a DuckDB oracle has one more)."""
    out = {}
    csv = root / "testdata" / "cost-registry.csv"
    lines = csv.read_text().splitlines() if csv.exists() else []
    header = lines[0].split(",") if lines else []
    if {"cpu_oracle", "gpu_oracle", "schema_validation"} <= set(header):
        at = [header.index(c) for c in ("dataset", "query", "cpu_oracle", "gpu_oracle",
                                          "schema_validation")]
        for line in lines[1:]:
            f = line.split(",")
            if len(f) == len(header) and f[at[2]] != "na":
                out[(f[at[0]], f[at[1]])] = (f[at[2]], f[at[3]], f[at[4]])
        return out
    inc = root / "peacockdb-core" / "tests" / "common" / "corpus_cases.inc"
    text = inc.read_text() if inc.exists() else ""
    for m in re.finditer(r"^corpus_query!\(([^)]*)\);", text, re.M):
        a = [x.strip() for x in m.group(1).split(",")]
        out[(a[0], a[2])] = (a[-3], a[-2], a[-1].replace("schema_validation_", ""))
    return out


def declared_cells(decl) -> tuple:
    """The two oracle cells and the schema validation cell, each its keyword shortened, the
    full keyword on hover."""
    if decl is None:
        dash = '<td class="decl">—</td>'
        return dash + dash, dash
    cpu, gpu, validation = decl
    short = lambda k: k.replace("data_fusion_", "").replace("_", " ")
    def cell(k, cls=""):
        tip = f"{k}: {ORACLE_HOW[k]}" if k in ORACLE_HOW else k
        return f'<td class="decl{cls}" title="{html.escape(tip)}">{html.escape(short(k))}</td>'
    return (cell(cpu) + cell(gpu),
            cell(validation, " off" if validation == "disabled" else ""))
GROUPS = [("Planning", 1), ("CPU execution", 2), ("GPU execution", 3)]
MODES = [("1s", "tp1-single"), ("1r", "tp1-rowgroup"), ("4s", "tp4-single"),
         ("4r", "tp4-rowgroup"), ("4z", "tp4-sized")]
LEGEND = ("<sub>One column per mode: " + ", ".join(f"{s} {m}" for s, m in MODES)
          + ". ✓ enabled, ✗ disabled, ~ skipped, — no test at that mode; one ✔ or — spans all five.</sub>")
GLYPH_CLASS = {"✓": "on", "✗": "off", "~": "skip", "—": "na"}


def section_index(root: Path, canon: str) -> dict:
    """(group, mode) -> (repo-relative file, {section name: line}). Plan and cpu sections are
    `== <query>` in one file per mode; the device's are `== <query> mode=<mode>` in one file."""
    files = {}
    for _, mode in MODES:
        files[(0, mode)] = f"{canon}/{mode}.plans.txt"
        files[(1, mode)] = f"{canon}/{mode}-mini.cpu.txt"
        files[(2, mode)] = f"{canon}/gpu-result.txt"
    index, read = {}, {}
    for key, rel in files.items():
        if rel not in read:
            path = root / rel
            lines = path.read_text().splitlines() if path.exists() else []
            read[rel] = {l[3:].strip(): at + 1 for at, l in enumerate(lines) if l.startswith("== ")}
        index[key] = (rel, read[rel])
    return index


def glyph_url(sections: dict, group: int, mode: str, stem: str, repo: str, blob: str):
    """The query's section for one mode of one group, or its file where it has none."""
    if not sections:
        return None
    rel, lines = sections[(group, mode)]
    line = lines.get(f"{stem} mode={mode}" if group == 2 else stem)
    return f"https://github.com/{repo}/blob/{blob}/{rel}" + (f"#L{line}" if line else "")


def glyph_cells(glyphs: str, urls: list) -> str:
    """One small cell per mode — or, where cost-report wrote one glyph for all five (a ✔
    enabled everywhere, a — with no test anywhere), one cell across the five."""
    def linked(g, url):
        return f'<a href="{html.escape(url)}">{g}</a>' if url else g

    if len(glyphs) == 1:
        cls = "on" if glyphs == "✔" else GLYPH_CLASS.get(glyphs, "na")
        return (f'<td class="g {cls}" colspan="{len(MODES)}" title="every mode">'
                f"{linked(glyphs, urls[-1])}</td>")
    out = []
    for i, ((short, mode), g) in enumerate(zip(MODES, glyphs.ljust(5, "—"))):
        edge = " first" if i == 0 else " last" if i == len(MODES) - 1 else ""
        out.append(f'<td class="g {GLYPH_CLASS.get(g, "na")}{edge}" title="{mode}">'
                   f"{linked(g, urls[i])}</td>")
    return "".join(out)
# Datasets whose queries that plan at no mode move to sections at the end of their table.
REFUSED_SECTIONS = {"TPC-DS"}


def ticket_index(wiki: Path, ref: str, index: dict) -> dict:
    """Add ticket number -> (repo-relative file, ref, short description, full title, closed) to
    `index`, the first file to anchor a number winning, as cost-report's own index does."""
    files = [wiki / "tickets.md", *sorted((wiki / "tickets").glob("*.md")),
             wiki / "archive" / "archived-tickets.md"]
    for path in files:
        if not path.exists():
            continue
        archived = path.parent.name == "archive"
        lines = path.read_text().splitlines()
        for at, line in enumerate(lines):
            anchor = ANCHOR.search(line)
            if not anchor:
                continue
            number = anchor.group(1)
            header = next((l for l in lines[at + 1:] if l.strip() and not ANCHOR.search(l)), "")
            m = HEADER.match(header.strip())
            if not m or m.group(1) != number or number in index:
                continue
            short = m.group(2) or ("closed" if archived else "")
            rel = f"llm-wiki/{path.relative_to(wiki).as_posix()}"
            index[number] = (rel, ref, short, m.group(3).strip(), archived)
    return index


def ticket_cell(numbers: str, index: dict, repo: str) -> str:
    items = []
    for token in numbers.split():
        n = token.lstrip("#")
        rel, ref, short, title, closed = index.get(n, (None, None, "", "not found in the wiki", False))
        if rel:
            url = f"https://github.com/{repo}/blob/{ref}/{rel}#t{n}"
            num = f'<a href="{html.escape(url)}" title="{html.escape(title)}">#{n}</a>'
        else:
            num = f'<span class="missing" title="{html.escape(title)}">#{n}</span>'
        desc = f' <span class="short">({html.escape(short)})</span>' if short else ""
        if closed and short != "closed":
            desc += ' <span class="short">(closed)</span>'
        item = f"<s>{num}{desc}</s>" if closed else f"{num}{desc}"
        items.append(f'<span class="ticket">{item}</span>')
    return '<td class="tickets">' + "<br>".join(items) + "</td>"


def text_of(cell: str) -> str:
    return re.sub(r"<[^>]+>", "", cell).strip()


def link_text(cell: str, url: str) -> str:
    """Wrap a cell's visible text in a link, keeping its <sub> and dropping any old link."""
    inner = re.sub(r"</?a[^>]*>", "", re.match(r"<td[^>]*>(.*)</td>", cell, re.S).group(1))
    text = text_of(inner)
    return "<td>" + inner.replace(text, f'<a href="{html.escape(url)}">{text}</a>', 1) + "</td>"


class Counts:
    """Per group: rows enabled at all five modes, at some of them, and the rows seen."""

    def __init__(self):
        self.full = [0, 0, 0]
        self.partial = [0, 0, 0]
        self.total = 0

    def add(self, glyph_cells):
        self.total += 1
        for g, glyphs in enumerate(glyph_cells):
            # A newer cost-report writes one ✔ for a cell enabled at every mode.
            enabled = 5 if glyphs == "✔" else glyphs.count("✓")
            if enabled == 5:
                self.full[g] += 1
            elif enabled:
                self.partial[g] += 1

    def html(self) -> str:
        lines = [f"<b>{name}:</b> {self.full[g]} fully / {self.partial[g]} partially / {self.total} total"
                 for g, (name, _) in enumerate(GROUPS)]
        return '<p class="counts">' + "<br>".join(lines) + "</p>"


def rewrite_table(label: str, slug: str, table: str, root: Path, blob: str, index: dict, repo: str,
                  blocked: dict, decls: dict):
    canon, queries = DATASETS.get(label, (None, None))
    counts = Counts()
    window, upgrade = [], []
    sections = section_index(root, canon) if canon else {}

    def row(m):
        cells = CELL.findall(m.group(0))
        if not cells or len(cells) < 6:
            return m.group(0)
        query = text_of(cells[0])
        stem = query.replace("_", "-")
        # A row that does not plan merges its three mode cells into one colspan cell.
        merged = "colspan" in cells[1]
        counts.add(["", "", ""] if merged else [text_of(c) for c in cells[1:4]])
        if merged:
            cells[1] = re.sub(r'colspan="\d+"', f'colspan="{3 * len(MODES)}"', cells[1])
        else:
            cells[1:4] = [glyph_cells(text_of(c), [glyph_url(sections, g, mode, stem, repo, blob)
                                                    for _, mode in MODES])
                          for g, c in enumerate(cells[1:4])]
        if queries and (root / queries / f"{stem}.sql").exists():
            cells[0] = link_text(cells[0], f"https://github.com/{repo}/blob/{blob}/{queries}/{stem}.sql")
        duck = 3 if merged else 5
        dk = root / canon / f"{stem}.duckdb_cost.txt" if canon else None
        if dk and dk.exists() and text_of(cells[duck]) not in ("", "—"):
            cells[duck] = link_text(cells[duck], f"https://github.com/{repo}/blob/{blob}/{canon}/{stem}.duckdb_cost.txt")
        last = text_of(cells[-1])
        anchor = f"{slug}-{stem}"
        numbers = last.replace("#", "").split() if TICKETS.fullmatch(last) else []
        if numbers:
            cells[-1] = ticket_cell(last, index, repo)
            for n in numbers:
                blocked.setdefault(n, []).append((label, query, anchor))
        oracles, validation = declared_cells(decls.get((REGISTRY_NAME.get(label), query)))
        cells[0] += oracles
        cells[1 if merged else 3] += validation
        out = f'<tr id="{anchor}">' + "".join(cells) + "</tr>"
        if label in REFUSED_SECTIONS and (merged or "✓" not in text_of(cells[1])
                                          and "✔" not in text_of(cells[1])):
            if "window_functions" in text_of(cells[-2]).split():
                window.append(out)
                return ""
            if any(index.get(n, ("",))[0].endswith("/df-upgrade.md") for n in numbers):
                upgrade.append(out)
                return ""
        return out

    table = re.sub(r"<tr><td.*?</tr>", row, table, flags=re.S)
    header = re.search(r"<tr><th>.*?</tr>", table, re.S).group(0)
    heads = re.findall(r"<th>.*?</th>", header, re.S)
    modes = "".join(
        f'<th class="m{" first" if i == 0 else " last" if i == len(MODES) - 1 else ""}" '
        f'title="{mode}">{short}</th>' for i, (short, mode) in enumerate(MODES)) * 3
    span = lambda h: h.replace("<th>", f'<th colspan="{len(MODES)}">')
    side = lambda t: f'<th rowspan="2"><sub>{t}</sub></th>'
    new_header = (heads[0].replace("<th>", '<th rowspan="2">')
                  + side("cpu oracle") + side("gpu oracle")
                  + "".join(span(h) for h in heads[1:4]) + side("schema validation")
                  + "".join(h.replace("<th>", '<th rowspan="2">') for h in heads[4:]))
    table = table.replace(header, f"<tr>{new_header}</tr>\n<tr>{modes}</tr>", 1)
    width = 1 + 2 + 3 * len(MODES) + 1 + len(heads) - 4
    tail = ""
    for title, rows in (("Window functions — refused at planning", window),
                        ("DataFusion upgrade — refused at planning, a different fix proposed",
                         upgrade)):
        if rows:
            tail += (f'<tr class="group"><th colspan="{width}">{title} ({len(rows)})</th></tr>\n'
                     + "\n".join(rows) + "\n")
    table = table.replace("</table>", tail + "</table>")
    return table, counts


def by_ticket_cell(n: str, index: dict, repo: str) -> str:
    """`joins #152 (streamed probe build)`, all of it one link to the ticket's body."""
    rel, ref, short, title, closed = index.get(n, (None, None, "", "not found in the wiki", False))
    name = Path(rel).stem if rel else "?"
    text = f"{name} #{n}" + (f" ({short})" if short else "")
    if closed and short != "closed":
        text += " (closed)"
    text = html.escape(text)
    if rel:
        url = f"https://github.com/{repo}/blob/{ref}/{rel}#t{n}"
        text = f'<a href="{html.escape(url)}" title="{html.escape(title)}">{text}</a>'
    else:
        text = f'<span class="missing">{text}</span>'
    return f'<td class="tickets">{"<s>" + text + "</s>" if closed else text}</td>'


def by_ticket(blocked: dict, labels: list, index: dict, repo: str) -> str:
    """One row per cited ticket, most queries first: the ticket, its title, and per dataset the
    queries whose row cites it, each linking to that row in its dataset's table."""
    rows = []
    for n in sorted(blocked, key=lambda n: (-len(blocked[n]), int(n))):
        title = html.escape(index.get(n, (None, None, "", "not found in the wiki", False))[3])
        cells = []
        for label in labels:
            qs = [(q, a) for l, q, a in blocked[n] if l == label]
            links = ", ".join(f'<a href="#{a}">{html.escape(q)}</a>' for q, a in qs)
            cells.append(f'<td class="blocked">{len(qs)}: {links}</td>' if qs else "<td></td>")
        rows.append(f"<tr>{by_ticket_cell(n, index, repo)}<td>{title}</td>"
                    + "".join(cells) + "</tr>")
    head = "".join(f"<th>{html.escape(l)}</th>" for l in labels)
    return ('<h2 id="by-ticket">By ticket</h2>\n<table>\n<tr><th>Ticket</th><th>Title</th>'
            + head + "</tr>\n" + "\n".join(rows) + "\n</table>")


def inline_markdown(text: str) -> str:
    """The few markdown forms the comment uses outside its HTML tables."""
    text = re.sub(r"\[([^\]]+)\]\(([^)]+)\)", r'<a href="\2">\1</a>', text)
    text = re.sub(r"\*\*([^*]+)\*\*", r"<b>\1</b>", text)
    return re.sub(r"(?<![\w/])_([^_\n]+)_(?![\w/])", r"<i>\1</i>", text)


def render(md: str, root: Path, blob: str, index: dict, repo: str, source: str) -> str:
    sections, toc, blocked, labels = [], [], {}, []
    decls = declarations(root)
    for m in SECTION.finditer(md):
        label = m.group("label")
        labels.append(label)
        slug = re.sub(r"[^a-z0-9]+", "-", label.lower()).strip("-")
        table, counts = rewrite_table(label, slug, m.group("table"), root, blob, index, repo,
                                      blocked, decls)
        toc.append(f'<li><a href="#{slug}">{html.escape(label)}</a> — {html.escape(m.group("summary"))}</li>')
        sections.append(f'<h2 id="{slug}">{html.escape(label)}</h2>\n{counts.html()}\n'
                         f'{LEGEND}\n{table}')
    prose = [inline_markdown(l) for l in SECTION.sub("", md).splitlines()
             if l.strip() and not l.startswith("<!--")]
    head, foot = prose[:-1], prose[-1:]
    return f"""<!doctype html>
<html><head><meta charset="utf-8"><title>PeacockDB coverage widget</title>
<style>
 body {{ font: 14px/1.4 -apple-system, "Segoe UI", sans-serif; margin: 1.5em; color: #1f2328; }}
 table {{ border-collapse: collapse; margin: .5em 0 1.5em; }}
 th, td {{ border: 1px solid #6e7781; padding: 3px 8px; vertical-align: top; }}
 /* Between the five mode sub-cells of one group the border is light; a group's outer edges
    and every other column keep the dark one. */
 td.g, th.m {{ border-left-color: #d8dee4; border-right-color: #d8dee4; }}
 td.g.first, th.m.first {{ border-left-color: #6e7781; }}
 td.g.last, th.m.last {{ border-right-color: #6e7781; }}
 td.g[colspan] {{ border-left-color: #6e7781; border-right-color: #6e7781; }}
 th {{ background: #f6f8fa; position: sticky; top: 0; }}
 tr:nth-child(even) td {{ background: #fafbfc; }}
 td.tickets {{ white-space: nowrap; font-size: 13px; }}
 .short {{ color: #57606a; }}
 .missing {{ color: #cf222e; }}
 .counts {{ margin: .2em 0 .6em; }}
 h2 {{ margin-top: 1.6em; border-bottom: 1px solid #d0d7de; }}
 a {{ color: #0969da; text-decoration: none; }}
 a:hover {{ text-decoration: underline; }}
 .source {{ color: #57606a; font-size: 12px; }}
 td.blocked {{ max-width: 28em; }}
 tr:target td {{ background: #fff8c5; }}
 td.g {{ text-align: center; padding: 3px 2px; min-width: 1.4em; font-size: 12px; }}
 td.g.on {{ background: #dafbe1; color: #1a7f37; }}
 td.g.off {{ background: #ffebe9; color: #cf222e; }}
 td.g.skip {{ background: #fff8c5; }}
 td.g.na {{ color: #8c959f; }}
 td.g a {{ color: inherit; display: block; }}
 td.decl {{ font-size: 12px; white-space: nowrap; color: #424a53; }}
 td.decl.off {{ background: #ffebe9; }}
 th.m {{ font-size: 11px; font-weight: normal; padding: 2px; }}
 tr.group th {{ text-align: left; background: #eaeef2; padding-top: 8px; }}
</style></head><body>
<p class="source">{html.escape(source)}</p>
{chr(10).join(f"<p>{l}</p>" for l in head)}
<p class="counts">Fully: enabled at all five modes. Partially: at one to four of them.</p>
<ul>
<li><a href="#by-ticket">By ticket</a> — {len(blocked)} tickets</li>
{chr(10).join(toc)}
</ul>
{by_ticket(blocked, labels, index, repo)}
{chr(10).join(sections)}
{chr(10).join(f"<p>{l}</p>" for l in foot)}
</body></html>
"""


def main() -> int:
    here = Path(__file__).resolve().parent.parent
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--root", type=Path, default=here, help="tree whose cost-report renders the widget")
    ap.add_argument("--wiki", action="append", default=[], metavar="PATH[=REF]",
                    help="wiki the tickets are read from, linked at REF (default --ref); repeat to "
                         "fall back to another, the first to hold a number winning "
                         "(default: <root>/llm-wiki)")
    ap.add_argument("--md", type=Path, help="an existing `cost-report --md` comment; skips cargo")
    ap.add_argument("--repo", default=DEFAULT_REPO)
    ap.add_argument("--ref", default="master", help="branch or sha the ticket links point at")
    ap.add_argument("--blob", default="master",
                    help="branch or sha the query, golden and DuckDB links point at")
    ap.add_argument("--out", type=Path,
                    help="the page to write (default: <root>/testdata/report.html, checked in)")
    args = ap.parse_args()

    root = args.root.resolve()
    blob = args.blob
    out = args.out or root / "testdata" / "report.html"

    def shown(path) -> str:
        """A path as the page prints it: repo-relative inside the tree, so the checked-in page
        does not change with the machine or checkout that wrote it."""
        p = Path(path).resolve()
        return p.relative_to(root).as_posix() if p.is_relative_to(root) else str(path)

    if args.md:
        md, source = args.md.read_text(), f"Tables from {shown(args.md)}"
    else:
        with tempfile.TemporaryDirectory() as tmp:
            md_path = Path(tmp) / "pr_comment.md"
            run = subprocess.run(
                ["cargo", "run", "-q", "-p", "cost-report", "--",
                 "--testdata", str(root / "testdata"),
                 "--html", str(Path(tmp) / "report.html"), "--md", str(md_path)],
                cwd=root, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
            if run.returncode != 0:
                sys.stderr.write(run.stderr)
                return run.returncode
            md = md_path.read_text()
        source = ("Generated by scripts/ticket_widget.py from cost-report's PR widget over "
                  "testdata/cost-registry.csv and the goldens")
    index = {}
    for spec in args.wiki or [str(root / "llm-wiki")]:
        path, _, ref = spec.partition("=")
        ticket_index(Path(path).resolve(), ref or args.ref, index)
        source += f" · tickets from {shown(path)} @ {ref or args.ref}"
    source += f" · query and golden links @ {blob}"
    out.write_text(render(md, root, blob, index, args.repo, source))
    cited = set(re.findall(r"#(\d+)", " ".join(
        text_of(c) for c in re.findall(r"<td>#\d+[^<]*</td></tr>", md))))
    missing = sorted(cited - index.keys(), key=int)
    print(f"{shown(out)}: {len(cited)} tickets cited, {len(missing)} not in the wiki"
          + (f": {' '.join('#' + n for n in missing)}" if missing else ""))
    return 0


if __name__ == "__main__":
    sys.exit(main())
