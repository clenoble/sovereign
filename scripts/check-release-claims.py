#!/usr/bin/env python3
"""Check that every claim of a release-notes file names the test that proves it.

Contract: doc/plans/CLAIMS-verifier.md (guarantees V1-V9, and the section
"Readings settled with the test author"). Tests: scripts/tests/.

    python -I scripts/check-release-claims.py RELEASE_NOTES_vX.Y.Z.md \
        --list cargo-list-core.txt [--list ...] [--root .]

A green run is not a proof: it says each claim names an existing test that the
gate ran, not that the test is relevant to the claim. The table printed here is
what a second position reads before publishing (V6).

Exit codes (V7): 0 every claim ok or no-claim; 1 any claim fails or any stray
marker; 2 usage error. Nothing is written anywhere; output on stdout only
(usage errors go to stderr).

Python 3 standard library only (V8).
"""

import argparse
import re
import sys
from pathlib import Path

# ---------------------------------------------------------------------------
# Output and input encodings
# ---------------------------------------------------------------------------


def _reconfigure_stdout():
    # Settled (contract, readings): stdout is UTF-8, set by the script itself,
    # since `-I` ignores PYTHONIOENCODING. newline="\n": no \r reaches stdout.
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
    except (AttributeError, ValueError):
        pass


class UsageError(Exception):
    """A usage error: exit 2 (V7)."""


def read_text_file(path, what):
    """Read a notes or list file as text, LF line endings.

    Settled (contract, readings): CRLF reads as LF; a UTF-16 BOM (a PowerShell
    5.1 `>` redirect) reads as UTF-16; anything else undecodable is a usage
    error (exit 2), not a file in which nothing was found."""
    try:
        data = Path(path).read_bytes()
    except OSError as e:
        raise UsageError(f"cannot read {what} {path}: {e.strerror or e}")
    try:
        if data.startswith(b"\xff\xfe") or data.startswith(b"\xfe\xff"):
            text = data.decode("utf-16")
        else:
            text = data.decode("utf-8-sig")
    except UnicodeDecodeError:
        raise UsageError(f"cannot decode {what} {path}: neither UTF-8 nor UTF-16 with a BOM")
    return text.replace("\r\n", "\n").replace("\r", "\n")


# ---------------------------------------------------------------------------
# Markers
# ---------------------------------------------------------------------------

# `<!-- test: path::name -->`, `<!-- manual: path §n -->`, `<!-- no-claim -->`.
MARKER_RE = re.compile(
    r"<!--\s*(?:(?P<kind>test|manual)\s*:(?P<body>.*?)|(?P<noclaim>no-claim)(?P<rest>.*?))-->"
)
# Any HTML comment (a line made only of non-marker comments is neutral).
COMMENT_ONLY_RE = re.compile(r"^\s*(?:<!--.*?-->\s*)+$")


class Marker:
    def __init__(self, line, raw, kind, body):
        self.line = line      # 1-based line number in the notes
        self.raw = raw        # the marker as written
        self.kind = kind      # "test", "manual" or "no-claim"
        self.body = body.strip()


def markers_on(line_text, lineno):
    out = []
    for m in MARKER_RE.finditer(line_text):
        if m.group("kind"):
            out.append(Marker(lineno, m.group(0), m.group("kind"), m.group("body")))
        else:
            out.append(Marker(lineno, m.group(0), "no-claim", m.group("rest")))
    return out


def is_marker_only(line_text):
    return bool(MARKER_RE.search(line_text)) and not MARKER_RE.sub("", line_text).strip()


# ---------------------------------------------------------------------------
# V1 — What is a claim
# ---------------------------------------------------------------------------

H1_RE = re.compile(r"^#\s")
HEADING_RE = re.compile(r"^\s{0,3}#{1,6}(?:\s|$)")
KNOWN_LIMITATIONS_RE = re.compile(r"^\s{0,3}##\s+Known limitations\s*#*\s*$", re.I)
BLOCKQUOTE_RE = re.compile(r"^\s{0,3}>")
THEMATIC_BREAK_RE = re.compile(r"^\s{0,3}([-*_])(?:\s*\1){2,}\s*$")
LIST_ITEM_RE = re.compile(r"^\s*(?:[-*+]|\d{1,9}[.)])(?:\s+|$)")
FENCE_RE = re.compile(r"^\s*(```|~~~)")


class Claim:
    def __init__(self, line):
        self.line = line     # its first line number: the location (settled)
        self.texts = []      # its text lines (marker-only lines excluded)
        self.markers = []


def parse_notes(text):
    """Split the notes into claims (V1).

    From the first line after the H1 up to `## Known limitations` (exclusive):
    every list item (any depth, each its own claim) and every plain paragraph
    is a claim. Headings, blockquotes, blank lines and thematic breaks are not
    claims. A list item's continuation lines belong to it.

    A marker belongs to the claim on whose lines it sits, or to the claim
    right before it with no blank line between (a marker-only line). Anywhere
    else it is a stray marker (settled readings).

    Paragraphs end as in CommonMark: a list item ends a paragraph even without
    a blank line (settled readings).

    Returns (claims, strays, limitations_line or None)."""
    lines = text.split("\n")
    start = 0
    for i, l in enumerate(lines):
        if H1_RE.match(l):
            start = i + 1
            break

    claims, strays = [], []
    current = None          # the open claim, which a following line may join
    in_fence = False
    limitations = None

    for i in range(start, len(lines)):
        n = i + 1
        l = lines[i]

        if in_fence:
            # Code inside a fence is text of the claim, never a marker.
            if current is not None:
                current.texts.append(l)
            if FENCE_RE.match(l):
                in_fence = False
            continue

        if KNOWN_LIMITATIONS_RE.match(l):
            # V1: `## Known limitations` and everything after are not checked.
            limitations = n
            break

        if not l.strip():
            current = None
            continue

        if HEADING_RE.match(l) or BLOCKQUOTE_RE.match(l) or THEMATIC_BREAK_RE.match(l):
            # Not claims; a marker here belongs to no claim: stray.
            strays.extend(markers_on(l, n))
            current = None
            continue

        if is_marker_only(l):
            if current is not None:
                current.markers.extend(markers_on(l, n))
            else:
                strays.extend(markers_on(l, n))
            continue

        if COMMENT_ONLY_RE.match(l):
            # A plain HTML comment (not a marker): neither a claim nor a break.
            continue

        if LIST_ITEM_RE.match(l) or current is None:
            current = Claim(n)
            claims.append(current)
        current.texts.append(l)
        current.markers.extend(markers_on(l, n))
        if FENCE_RE.match(l):
            in_fence = True

    return claims, strays, limitations


def first_words(claim, width=60):
    """V6: the claim's first words, about 60 characters."""
    text = " ".join(claim.texts)
    text = MARKER_RE.sub(" ", text)
    text = re.sub(r"^\s*(?:[-*+]|\d{1,9}[.)])\s+", "", text)
    text = text.replace("**", "").replace("__", "")
    text = " ".join(text.split())
    if len(text) <= width:
        return text
    cut = text[:width]
    if " " in cut:
        cut = cut[: cut.rindex(" ")]
    return cut.rstrip(" ,;:") + "…"


# ---------------------------------------------------------------------------
# Gate lists (V4)
# ---------------------------------------------------------------------------


class GateLists:
    def __init__(self, texts):
        self.rust = set()
        self.frontend = set()
        for text in texts:
            for raw in text.split("\n"):
                line = raw.strip()
                if not line:
                    continue
                # V4: a Rust test is run when a line ends in `<name>: test`,
                # with <name> the last `::` segment.
                if line.endswith(": test"):
                    self.rust.add(line[: -len(": test")].strip().split("::")[-1])
                # V4: a frontend test is run when its title appears in a
                # listing: vitest prints `file > suite > title`.
                self.frontend.add(line)
                self.frontend.add(line.split(" > ")[-1].strip())

    def rust_ran(self, name):
        return name in self.rust

    def frontend_ran(self, title):
        return title in self.frontend


# ---------------------------------------------------------------------------
# V3 — A named test exists, as a test, not ignored (Rust)
# ---------------------------------------------------------------------------

TEST_ATTR_RE = re.compile(r"^#\[\s*(?:test\s*\]|tokio\s*::\s*test\b)")
IGNORE_ATTR_RE = re.compile(r"^#\[\s*ignore\b")
INLINE_ATTR_RE = re.compile(r"#\[[^\]]*\]")


def _attributes_before(lines, fn_index, prefix):
    """The attributes that apply to the fn on line `fn_index`.

    V3: attributes and doc comments only in between; settled readings: an
    attribute may span several lines, and line comments may sit between the
    attributes and the fn. Walks upward and stops at anything else (another
    item, a blank line)."""
    attrs = [a for a in INLINE_ATTR_RE.findall(prefix)]
    i = fn_index - 1
    while i >= 0:
        s = lines[i].strip()
        if s.startswith("//"):
            i -= 1
            continue
        if s.startswith("#[") and s.endswith("]"):
            attrs.append(s)
            i -= 1
            continue
        if s.endswith("]"):
            # The end of an attribute spread over several lines: gather up to
            # the line that opens it.
            j = i
            while j >= 0 and not lines[j].strip().startswith("#[") and i - j < 40:
                j -= 1
            if j < 0 or not lines[j].strip().startswith("#["):
                break
            joined = " ".join(x.strip() for x in lines[j : i + 1])
            if joined.count("[") != joined.count("]"):
                break
            attrs.append(joined)
            i = j - 1
            continue
        break
    return attrs


def check_rust(path, name, source, gate):
    lines = source.split("\n")
    fn_re = re.compile(r"\bfn\s+" + re.escape(name) + r"\b\s*[(<]")
    verdicts = []
    for idx, l in enumerate(lines):
        m = fn_re.search(l)
        if not m:
            continue
        prefix = l[: m.start()]
        if "//" in prefix:
            continue  # inside a comment
        attrs = _attributes_before(lines, idx, prefix)
        is_test = any(TEST_ATTR_RE.match(a) for a in attrs)
        ignored = any(IGNORE_ATTR_RE.match(a) for a in attrs)
        if is_test and not ignored:
            verdicts.append("test")
        elif is_test:
            verdicts.append("ignored")
        else:
            verdicts.append("not a test")
    if not verdicts:
        return "no such function"
    # Settled readings: V3's reason comes before V4's.
    if "test" not in verdicts:
        return "ignored" if "ignored" in verdicts else "not a test"
    if not gate.rust_ran(name):
        return "not run by the gate"
    return "ok"


# ---------------------------------------------------------------------------
# V3 — A named test exists, as a test, not skipped (frontend)
# ---------------------------------------------------------------------------


def check_frontend(path, title, source, gate):
    # V3: `it(` or `test(` with that exact title (single or double quotes,
    # settled readings), and not `.skip` / `.todo` / `xit(`.
    call_re = re.compile(
        r"(?<![\w$.])(x?it|x?test)((?:\s*\.\s*[A-Za-z]+)*)\s*\(\s*(['\"`])"
        + re.escape(title)
        + r"\3"
    )
    found = []
    for m in call_re.finditer(source):
        fn, mods = m.group(1), re.sub(r"\s", "", m.group(2)).split(".")
        skipped = fn.startswith("x") or "skip" in mods or "todo" in mods
        found.append("skipped" if skipped else "test")
    if not found:
        # Settled readings: a missing frontend title says "no such test".
        return "no such test"
    if "test" not in found:
        return "skipped"  # settled readings: `.todo` reads as skipped
    if not gate.frontend_ran(title):
        return "not run by the gate"
    return "ok"


# ---------------------------------------------------------------------------
# Proofs
# ---------------------------------------------------------------------------


def check_test_marker(marker, root, gate):
    """Returns (label, state): state is "ok" or the reason."""
    body = marker.body
    if "::" not in body:
        return f"test {body}", "malformed marker (no '::' in test:)"
    path, name = (x.strip() for x in body.split("::", 1))
    label = f"test {path}::{name}"
    if not path or not name:
        return label, "malformed marker (empty path or name)"
    # Settled readings: Rust or frontend is decided by the file extension.
    suffix = Path(path).suffix
    if suffix not in (".rs", ".ts", ".js"):
        return label, "malformed marker (test file must be .rs, .ts or .js)"
    f = root.joinpath(*path.split("/"))  # V8: paths in markers use `/`
    if not f.is_file():
        return label, "no such file"
    try:
        source = f.read_text(encoding="utf-8", errors="replace").replace("\r\n", "\n")
    except OSError:
        return label, "no such file (unreadable)"
    if suffix == ".rs":
        return label, check_rust(path, name, source, gate)
    return label, check_frontend(path, name, source, gate)


MANUAL_RE = re.compile(r"^(?P<path>\S.*?)\s+§\s*(?P<n>\S+)$")


def check_manual_marker(marker, root):
    # V5: the checklist file exists and holds item <n>: a heading or a list
    # item starting with <n> or §<n>. Settled readings: §n is a whole number.
    body = marker.body
    m = MANUAL_RE.match(body)
    if not m:
        return f"manual {body}", "malformed marker (no '§' in manual:)"
    path, n = m.group("path"), m.group("n")
    label = f"manual {path} §{n}"
    if not n.isdigit():
        return label, "malformed marker (§ must be followed by a whole number)"
    f = root.joinpath(*path.split("/"))
    if not f.is_file():
        return label, "no such file"
    try:
        text = f.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return label, "no such file (unreadable)"
    num = re.escape(str(int(n))) if n.lstrip("0") else "0"
    # The number ends at `.`, `)`, `:`, a space or the line's end, and a `.`
    # is not followed by another digit (§3 is not item 3.5; §1 is not 12.).
    end = r"(?:[.)](?!\d)|:|\s|$)"
    item_res = [
        re.compile(r"^\s{0,3}#{1,6}\s+(?:§\s*)?0*" + num + end),                  # heading
        re.compile(r"^\s*0*" + num + r"[.)](?:\s|$)"),                             # ordered item
        re.compile(r"^\s*[-*+]\s+(?:\[[ xX]\]\s+)?(?:§\s*)?0*" + num + end),      # bullet item
        re.compile(r"^\s*§\s*0*" + num + end),                                     # bare §n
    ]
    for line in text.replace("\r\n", "\n").split("\n"):
        if any(r.match(line) for r in item_res):
            return label, "ok"
    return label, f"no item §{n} in the checklist"


def evaluate(claim, root, gate):
    """Returns (state, proofs): state is "ok", "no-claim" or "FAIL";
    proofs is a list of (label, state-or-reason)."""
    proofs = []
    noclaim = False
    for mk in claim.markers:
        if mk.kind == "test":
            proofs.append(check_test_marker(mk, root, gate))
        elif mk.kind == "manual":
            proofs.append(check_manual_marker(mk, root))
        elif mk.body:
            proofs.append((mk.raw, "malformed marker (no-claim takes nothing after it)"))
        else:
            noclaim = True
    if not proofs:
        if noclaim:
            return "no-claim", [("no-claim", None)]
        # V2: a claim with no test:, manual: or no-claim marker fails.
        return "FAIL", [("", "no marker")]
    if noclaim:
        proofs.append(("no-claim", "malformed marker (no-claim beside a proof)"))
    state = "ok" if all(s == "ok" for _, s in proofs) else "FAIL"
    return state, proofs


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def build_parser():
    p = argparse.ArgumentParser(
        prog="check-release-claims.py",
        description="Check that every release-note claim names a test the gate ran "
        "(doc/plans/CLAIMS-verifier.md).",
    )
    p.add_argument("notes", help="the release notes (Markdown)")
    p.add_argument(
        "--list", dest="lists", action="append", metavar="FILE",
        help="a test listing the gate ran (`cargo test ... -- --list`, the frontend "
        "runner's listing); repeatable; at least one is required",
    )
    p.add_argument("--root", help="the repo root (default: the script's parent's parent)")
    return p


def main(argv=None):
    _reconfigure_stdout()
    parser = build_parser()
    args = parser.parse_args(argv)
    # V4: without any --list, the verifier refuses to run, so the gate check
    # cannot be skipped by forgetting the argument.
    if not args.lists:
        parser.error("at least one --list FILE is required (V4: the gate's test listings)")

    root = Path(args.root) if args.root else Path(__file__).resolve().parent.parent
    try:
        notes_text = read_text_file(args.notes, "notes")
        gate = GateLists(read_text_file(lst, "list file") for lst in args.lists)
    except UsageError as e:
        print(f"check-release-claims: {e}", file=sys.stderr)
        return 2
    if not root.is_dir():
        print(f"check-release-claims: --root {root} is not a directory", file=sys.stderr)
        return 2

    claims, strays, limitations = parse_notes(notes_text)

    print(f"notes: {args.notes}")
    print(f"root:  {root}")
    if limitations is None:
        # V1, settled readings: a notice, not a failure.
        print("notice: heading `## Known limitations` not found; every claim to the end is checked")
    else:
        print(f"checked up to `## Known limitations` (line {limitations}), which is not checked")
    print()

    counts = {"ok": 0, "failed": 0, "no-claim": 0}
    # V6: one line per claim: location, first words, each proof, state.
    print(f"{'line':>5}  {'state':<8}  {'claim':<61}  proofs")
    for c in claims:
        state, proofs = evaluate(c, root, gate)
        if state == "ok":
            counts["ok"] += 1
        elif state == "no-claim":
            counts["no-claim"] += 1
        else:
            counts["failed"] += 1
        parts = []
        for label, s in proofs:
            if s is None:
                parts.append(label)
            elif not label:
                parts.append(s)
            else:
                parts.append(f"{label} [{s}]")
        print(f"{c.line:>5}  {state:<8}  {first_words(c):<61}  {'; '.join(parts)}")

    # Settled now (implementer's brief): a stray marker is not a claim. It gets
    # its own line, does not change the counts, and makes the run fail.
    if strays:
        print()
        for mk in strays:
            print(f"{mk.line:>5}  stray marker, on no claim: {mk.raw}")

    print()
    total = counts["ok"] + counts["failed"] + counts["no-claim"]
    summary = f"claims: {total}   ok: {counts['ok']}   failed: {counts['failed']}   no-claim: {counts['no-claim']}"
    if strays:
        summary += f"   stray: {len(strays)}"
    print(summary)
    print("A green run is not a proof: read the table (does each test prove its claim?).")

    # V7: 0 when every claim is ok or no-claim; 1 when any claim fails (or a
    # stray marker exists).
    return 1 if counts["failed"] or strays else 0


if __name__ == "__main__":
    sys.exit(main())
