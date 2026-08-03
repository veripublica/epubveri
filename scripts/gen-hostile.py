#!/usr/bin/env python3
"""Generate EPUBs designed to break the validator rather than to be validated.

Why this exists. The two instruments this project trusts — the epubcheck
corpus and the real-book shelf — both answer one question: *what verdict do
we give a well-formed book?* Neither can see a crash, a resource-exhaustion
bug, or a performance cliff, because none of those inputs is a well-formed
book. On 2026-08-03 a security review found six ways an ordinary `.epub`
could kill the process that validated it, and the corpus was byte-identical
and the shelf unchanged per book through every single one.

So the adversarial input has to be written by hand, and it is worth keeping:
the guards themselves have unit tests, but these generators are what would
find the *next* class. Each shape below is one that actually aborted or
exhausted a shipped release.

Usage:
    scripts/gen-hostile.py                  # write to target/hostile/
    scripts/gen-hostile.py --out DIR
    scripts/gen-hostile.py --scale          # also the manifest-size ladder

`target/` is gitignored, so the output is ephemeral by default and no EPUB
is ever committed.

Run `scripts/check-hostile.sh` afterwards; it is the half that decides
whether any of these still wins.
"""

import argparse
import os
import struct
import sys
import zipfile
import zlib

CONTAINER = (
    '<?xml version="1.0"?><container version="1.0" '
    'xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles>'
    '<rootfile full-path="OEBPS/c.opf" '
    'media-type="application/oebps-package+xml"/></rootfiles></container>'
)

NAV = (
    '<?xml version="1.0"?><html xmlns="http://www.w3.org/1999/xhtml" '
    'xmlns:epub="http://www.idpf.org/2007/ops"><head><title>n</title>{head}'
    "</head><body><nav epub:type=\"toc\"><ol><li><a href=\"n.xhtml\">n</a>"
    "</li></ol></nav>{body}</body></html>"
)


def opf(items="", spine="", version="3.0", dcterms=True):
    modified = (
        '<meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>'
        if dcterms
        else ""
    )
    return (
        f'<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" '
        f'version="{version}" unique-identifier="i"><metadata '
        'xmlns:dc="http://purl.org/dc/elements/1.1/">'
        '<dc:identifier id="i">x</dc:identifier><dc:title>t</dc:title>'
        f"<dc:language>en</dc:language>{modified}</metadata><manifest>"
        '<item id="n" href="n.xhtml" media-type="application/xhtml+xml" '
        f'properties="nav"/>{items}</manifest><spine>'
        f"<itemref idref=\"n\"/>{spine}</spine></package>"
    )


def write_epub(path, files, opf_xml, nav_head="", nav_body=""):
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("mimetype", "application/epub+zip", zipfile.ZIP_STORED)
        z.writestr("META-INF/container.xml", CONTAINER)
        z.writestr("OEBPS/c.opf", opf_xml)
        z.writestr("OEBPS/n.xhtml", NAV.format(head=nav_head, body=nav_body))
        for name, data in files.items():
            z.writestr(name, data)


def tiny_png():
    def chunk(tag, data):
        body = tag + data
        return struct.pack(">I", len(data)) + body + struct.pack(
            ">I", zlib.crc32(body) & 0xFFFFFFFF
        )

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(b"\x00\xff\xff\xff"))
        + chunk(b"IEND", b"")
    )


# --- the shapes, each with the failure it used to cause ----------------------


def gen_xml_depth(out, depth=50_000):
    """Stack overflow in roxmltree's mutually recursive tokenizer.

    Aborted at ~15,000 deep on an 8 MiB main thread and ~4,000 on a 2 MiB
    worker, from a file of about 1.1 KB. SIGABRT, not a catchable panic, so
    an embedder could not defend against it. Guard: ocf::MAX_XML_DEPTH.
    """
    body = "<div>" * depth + "x" + "</div>" * depth
    write_epub(
        os.path.join(out, "xml-depth.epub"), {}, opf(), nav_body=body
    )


def gen_zip_bomb(out, mib=400):
    """Unbounded inflation of one compressed entry.

    A 400 KB EPUB drove 1.3 GB of peak RSS and still reported VALID, because
    the read was a bare read_to_end. Guard: ocf::MAX_ENTRY_BYTES, reported as
    LIM-001.
    """
    big = (
        '<?xml version="1.0"?><html xmlns="http://www.w3.org/1999/xhtml">'
        "<head><title>b</title></head><body><p>"
        + "A" * (mib * 1024 * 1024)
        + "</p></body></html>"
    )
    write_epub(
        os.path.join(out, "zip-bomb.epub"),
        {"OEBPS/big.xhtml": big},
        opf(
            '<item id="b" href="big.xhtml" media-type="application/xhtml+xml"/>',
            '<itemref idref="b"/>',
        ),
    )


CSS_SHAPES = {
    # Each reaches the same mutual recursion in styloria's parser
    # (consume_component_value <-> consume_simple_block) by a different route.
    # All four aborted between 10,000 and 20,000 deep on an 8 MiB main thread,
    # from stylesheets of about 1.2 KB. Guard: styloria::MAX_NESTING_DEPTH,
    # reported as CSS-008 / css.stylesheet.nesting_too_deep.
    "paren": lambda n: "a{color:" + "(" * n + "red" + ")" * n + "}",
    "curly": lambda n: "@media all{" * n + "a{color:red}" + "}" * n,
    "function": lambda n: "a{color:" + "rgb(" * n + "1" + ")" * n + "}",
    "selector": lambda n: ":is(" * n + "a" + ")" * n + "{color:red}",
}


def gen_css_nesting(out, depth=100_000):
    for name, make in CSS_SHAPES.items():
        write_epub(
            os.path.join(out, f"css-{name}.epub"),
            {"OEBPS/s.css": make(depth)},
            opf('<item id="s" href="s.css" media-type="text/css"/>'),
            nav_head='<link rel="stylesheet" href="s.css" type="text/css"/>',
        )


def gen_xxe(out):
    """External-entity reference. Never exploitable — roxmltree does no I/O —
    but cheap to keep honest about, and the shape a reviewer will ask for."""
    path = os.path.join(out, "xxe.epub")
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("mimetype", "application/epub+zip", zipfile.ZIP_STORED)
        z.writestr("META-INF/container.xml", CONTAINER)
        z.writestr(
            "OEBPS/c.opf",
            '<?xml version="1.0"?><!DOCTYPE package [<!ENTITY xxe SYSTEM '
            '"file:///etc/passwd">]><package '
            'xmlns="http://www.idpf.org/2007/opf" version="3.0" '
            'unique-identifier="i"><metadata '
            'xmlns:dc="http://purl.org/dc/elements/1.1/">'
            '<dc:identifier id="i">&xxe;</dc:identifier><dc:title>t</dc:title>'
            "<dc:language>en</dc:language>"
            '<meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>'
            '</metadata><manifest><item id="n" href="n.xhtml" '
            'media-type="application/xhtml+xml" properties="nav"/></manifest>'
            '<spine><itemref idref="n"/></spine></package>',
        )
        z.writestr("OEBPS/n.xhtml", NAV.format(head="", body=""))


def gen_entry_count(out, n=100_000):
    """Many tiny ZIP entries. Measured linear and fast — kept so that stays
    true, not because it ever failed."""
    write_epub(
        os.path.join(out, "entry-count.epub"),
        {f"OEBPS/pad{i}.txt": "x" for i in range(n)},
        opf(),
    )


def gen_scale(out, sizes=(1000, 2000, 4000, 8000)):
    """The manifest-size ladder that exposed the quadratic.

    Every resource is referenced from the content document, so no finding is
    produced and the cost being measured is pure validation. Before 0.9.1 this
    was 42.6s at 4,000 items; it is now linear, so a future regression shows
    up as the ladder bending rather than as any one number.
    """
    png = tiny_png()
    for n in sizes:
        items = "".join(
            f'<item id="p{i}" href="p{i}.png" media-type="image/png"/>'
            for i in range(n)
        )
        body = "".join(f'<p><img src="p{i}.png" alt="a"/></p>' for i in range(n))
        write_epub(
            os.path.join(out, f"scale-{n}.epub"),
            {f"OEBPS/p{i}.png": png for i in range(n)},
            opf(items),
            nav_body=body,
        )


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--out", default="target/hostile")
    ap.add_argument(
        "--scale",
        action="store_true",
        help="also emit the manifest-size ladder (slower, ~50 MB)",
    )
    args = ap.parse_args()

    os.makedirs(args.out, exist_ok=True)
    gen_xml_depth(args.out)
    gen_zip_bomb(args.out)
    gen_css_nesting(args.out)
    gen_xxe(args.out)
    gen_entry_count(args.out)
    if args.scale:
        gen_scale(args.out)

    made = sorted(f for f in os.listdir(args.out) if f.endswith(".epub"))
    for f in made:
        size = os.path.getsize(os.path.join(args.out, f))
        print(f"  {size:>10,} B  {f}")
    print(f"{len(made)} file(s) in {args.out}", file=sys.stderr)


if __name__ == "__main__":
    main()
