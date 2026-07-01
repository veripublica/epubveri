#!/usr/bin/env python3
"""epubveri measurement spike.

Builds a set of synthetic EPUB fixtures (one valid + several each deliberately
tripping ONE high-value check), runs the built `epubveri` binary against them,
and reports coverage: did we catch the issue each fixture is designed to expose?

This measures the *mechanism* against hand-built fixtures. Wiring the real
epubcheck / W3C `epub-tests` corpora (each case carries an expected message ID)
is the next step and only needs fetching those repos.

Usage: python3 scripts/spike.py [path-to-epubveri-binary]
"""
import os
import subprocess
import sys
import zipfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FIX = os.path.join(ROOT, "tests", "fixtures")
BIN = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "target", "release", "epubveri")

S, D = zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED

CONTAINER = (
    '<?xml version="1.0"?>\n'
    '<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">\n'
    '  <rootfiles>\n'
    '    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>\n'
    '  </rootfiles>\n'
    '</container>\n'
)

def opf(version='3.0', uid='pub-id', ident=True, title=True, lang=True,
        manifest=None, spine=None, spine_toc=None):
    md = ['  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">']
    if ident:
        md.append('    <dc:identifier id="pub-id">urn:uuid:12345</dc:identifier>')
    if title:
        md.append('    <dc:title>Test Book</dc:title>')
    if lang:
        md.append('    <dc:language>en</dc:language>')
    md.append('  </metadata>')
    if manifest is None:
        manifest = [
            '    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>',
            '    <item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>',
        ]
    if spine is None:
        spine = ['    <itemref idref="ch1"/>']
    ver = f' version="{version}"' if version is not None else ''
    uidattr = f' unique-identifier="{uid}"' if uid is not None else ''
    toc = f' toc="{spine_toc}"' if spine_toc else ''
    return (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        f'<package xmlns="http://www.idpf.org/2007/opf"{ver}{uidattr}>\n'
        + '\n'.join(md) + '\n'
        '  <manifest>\n' + '\n'.join(manifest) + '\n  </manifest>\n'
        f'  <spine{toc}>\n' + '\n'.join(spine) + '\n  </spine>\n'
        '</package>\n'
    )

NAV = (
    '<?xml version="1.0" encoding="utf-8"?>\n'
    '<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">\n'
    '<head><title>Nav</title></head>\n'
    '<body><nav epub:type="toc"><ol><li><a href="chapter1.xhtml">Ch1</a></li></ol></nav></body>\n'
    '</html>\n'
)

def chapter(body='<h1>Chapter 1</h1><p>Hello.</p>'):
    return (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        '<html xmlns="http://www.w3.org/1999/xhtml">\n'
        f'<head><title>Chapter 1</title></head>\n<body>{body}</body>\n</html>\n'
    )

def base_entries(mimetype=b'application/epub+zip', mime_comp=S):
    """A fully valid EPUB 3 as an ordered entry list."""
    return [
        ('mimetype', mimetype, mime_comp),
        ('META-INF/container.xml', CONTAINER, D),
        ('OEBPS/content.opf', opf(), D),
        ('OEBPS/nav.xhtml', NAV, D),
        ('OEBPS/chapter1.xhtml', chapter(), D),
    ]

def put(entries, name, data, comp=D):
    """Replace (in place, preserving order) or append an entry."""
    out = []
    found = False
    for n, d, c in entries:
        if n == name:
            out.append((name, data, comp)); found = True
        else:
            out.append((n, d, c))
    if not found:
        out.append((name, data, comp))
    return out

def drop(entries, name):
    return [(n, d, c) for (n, d, c) in entries if n != name]

def build(path, entries):
    with zipfile.ZipFile(path, 'w') as z:
        for name, data, comp in entries:
            if isinstance(data, str):
                data = data.encode('utf-8')
            zi = zipfile.ZipInfo(name)
            zi.compress_type = comp
            z.writestr(zi, data)

# -- fixtures: (filename, expected_id_or_None, entries) --
def fixtures():
    fx = []
    fx.append(('valid.epub', None, base_entries()))

    # OCF / mimetype
    fx.append(('mimetype_compressed.epub', 'PKG-007',
               base_entries(mime_comp=D)))
    fx.append(('mimetype_wrong.epub', 'PKG-007',
               put(base_entries(), 'mimetype', b'application/zip', S)))
    e = base_entries()
    e = [e[1], e[0]] + e[2:]  # container before mimetype
    fx.append(('mimetype_not_first.epub', 'PKG-006', e))

    # container.xml
    fx.append(('no_container.epub', 'RSC-002', drop(base_entries(), 'META-INF/container.xml')))
    fx.append(('bad_container_xml.epub', 'RSC-005',
               put(base_entries(), 'META-INF/container.xml', '<container><rootfiles>')))
    fx.append(('no_rootfile.epub', 'RSC-003', put(
        base_entries(), 'META-INF/container.xml',
        '<?xml version="1.0"?><container version="1.0" '
        'xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles/></container>')))

    # OPF presence / well-formedness / version
    fx.append(('opf_missing.epub', 'OPF-002', drop(base_entries(), 'OEBPS/content.opf')))
    fx.append(('opf_malformed.epub', 'RSC-005',
               put(base_entries(), 'OEBPS/content.opf', '<?xml version="1.0"?><package><metadata>')))
    fx.append(('opf_no_version.epub', 'OPF-001',
               put(base_entries(), 'OEBPS/content.opf', opf(version=None))))

    # required metadata
    fx.append(('missing_title.epub', 'RSC-005',
               put(base_entries(), 'OEBPS/content.opf', opf(title=False))))
    fx.append(('missing_language.epub', 'RSC-005',
               put(base_entries(), 'OEBPS/content.opf', opf(lang=False))))
    fx.append(('missing_identifier.epub', 'RSC-005',
               put(base_entries(), 'OEBPS/content.opf', opf(ident=False))))

    # manifest / spine integrity
    fx.append(('manifest_href_missing.epub', 'RSC-001',
               put(base_entries(), 'OEBPS/content.opf', opf(manifest=[
                   '    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>',
                   '    <item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>',
                   '    <item id="gone" href="missing.xhtml" media-type="application/xhtml+xml"/>',
               ]))))
    fx.append(('manifest_no_mediatype.epub', 'RSC-005',
               put(base_entries(), 'OEBPS/content.opf', opf(manifest=[
                   '    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>',
                   '    <item id="ch1" href="chapter1.xhtml"/>',
               ]))))
    fx.append(('dup_manifest_id.epub', 'RSC-005',
               put(base_entries(), 'OEBPS/content.opf', opf(manifest=[
                   '    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>',
                   '    <item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>',
                   '    <item id="ch1" href="nav.xhtml" media-type="application/xhtml+xml"/>',
               ]))))
    fx.append(('spine_empty.epub', 'OPF-033',
               put(base_entries(), 'OEBPS/content.opf', opf(spine=[]))))
    fx.append(('spine_unresolved.epub', 'OPF-049',
               put(base_entries(), 'OEBPS/content.opf', opf(spine=['    <itemref idref="nope"/>']))))

    # EPUB 3 nav doc
    fx.append(('nav_missing.epub', 'RSC-005',
               put(base_entries(), 'OEBPS/content.opf', opf(manifest=[
                   '    <item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>',
               ]))))

    # broken internal reference in content
    fx.append(('broken_ref.epub', 'RSC-001',
               put(base_entries(), 'OEBPS/chapter1.xhtml',
                   chapter('<p><img src="missing.png" alt="x"/></p>'))))

    # duplicate spine reference -> OPF-034
    fx.append(('duplicate_spine.epub', 'OPF-034',
               put(base_entries(), 'OEBPS/content.opf',
                   opf(spine=['    <itemref idref="ch1"/>',
                              '    <itemref idref="ch1"/>']))))

    # EPUB 2 spine with no NCX 'toc' attribute -> RSC-005
    fx.append(('epub2_no_toc.epub', 'RSC-005',
               put(base_entries(), 'OEBPS/content.opf',
                   opf(version='2.0', manifest=[
                       '    <item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>',
                   ]))))

    # spine 'toc' pointing to a non-NCX resource -> OPF-050
    fx.append(('toc_non_ncx.epub', 'OPF-050',
               put(base_entries(), 'OEBPS/content.opf',
                   opf(version='2.0', spine_toc='ch1', manifest=[
                       '    <item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>',
                   ]))))

    # encrypted resource declared in encryption.xml -> RSC-004 (INFO)
    enc_xml = (
        '<?xml version="1.0"?>\n'
        '<encryption xmlns="urn:oasis:names:tc:opendocument:xmlns:container" '
        'xmlns:enc="http://www.w3.org/2001/04/xmlenc#">\n'
        '  <enc:EncryptedData>\n'
        '    <enc:CipherData><enc:CipherReference URI="OEBPS/chapter1.xhtml"/></enc:CipherData>\n'
        '  </enc:EncryptedData>\n'
        '</encryption>\n'
    )
    fx.append(('encryption.epub', 'RSC-004',
               put(base_entries(), 'META-INF/encryption.xml', enc_xml)))
    return fx


def run_ids(path):
    p = subprocess.run([BIN, '--format', 'ids', path], capture_output=True, text=True)
    ids = [ln.strip() for ln in p.stdout.splitlines() if ln.strip()]
    return ids, p.returncode


def main():
    if not os.path.exists(BIN):
        print(f"binary not found: {BIN}\nbuild first: cargo build --release", file=sys.stderr)
        return 1
    os.makedirs(FIX, exist_ok=True)
    fx = fixtures()
    covered = 0
    rows = []
    for name, expected, entries in fx:
        path = os.path.join(FIX, name)
        build(path, entries)
        ids, rc = run_ids(path)
        if expected is None:  # valid fixture: must be clean
            ok = (rc == 0 and not ids)
        else:  # the designed message ID must be reported (errors or INFO like RSC-004)
            ok = (expected in ids)
        covered += ok
        rows.append((name, expected or '(valid)', 'yes' if ok else 'NO', ','.join(ids) or '-'))

    w = max(len(r[0]) for r in rows)
    print(f"\n{'fixture':<{w}}  {'expected':<10}  {'caught':<6}  detected")
    print('-' * (w + 40))
    for name, exp, ok, det in rows:
        print(f"{name:<{w}}  {exp:<10}  {ok:<6}  {det}")
    total = len(fx)
    pct = 100.0 * covered / total
    print('-' * (w + 40))
    print(f"\nCoverage: {covered}/{total} fixtures = {pct:.1f}%\n")
    return 0 if covered == total else 2


if __name__ == '__main__':
    sys.exit(main())
