# Security policy

## Reporting a vulnerability

**Please do not open a public issue for a security problem.** Report it
privately through GitHub:
[**Report a vulnerability**](https://github.com/veripublica/epubveri/security/advisories/new)
(the *Security* tab of this repository). Only the maintainer can see the
report.

A useful report includes the smallest input that shows the problem (an EPUB,
or the file inside it that matters), the epubveri version (`epubveri -V`),
and what happened: the crash message, the memory or time used, or what was
read or written.

## What counts

epubveri reads files that other people made. That makes these security
problems, in the library, the CLI and the WASM package alike:

- **a crash**: a panic, an abort or a stack overflow on any input;
- **resource exhaustion out of proportion to the input**: memory or time
  that a small file can drive far past what its size justifies;
- **any file read or written other than the input**, and **any network
  access**. epubveri makes none by design, so any would be a defect;
- **the release pipeline**: anything that could put code into a published
  binary, crate or npm package that is not in this repository.

A wrong validation result (a false error or a missed one) is not a security
problem. Please report it as an ordinary
[issue](https://github.com/veripublica/epubveri/issues), where it helps
everyone who meets it.

The editor plugins are a separate project. Report problems in them to
[veripublica/epubveri-plugins](https://github.com/veripublica/epubveri-plugins).

## Supported versions

epubveri is before 1.0, so only the **latest release** receives fixes. A fix
ships as a new release, never as a patch to an old one.

## What happens next

epubveri has one maintainer, so these are aims, not guarantees:

- an acknowledgement within a week;
- a fix released **before** the details are public. The release's
  `CHANGELOG.md` entry then describes the problem under **Security**;
- credit to the reporter in that entry, if they want it.

## Verifying what you download

Every release is built and published by this repository's GitHub Actions
workflows. No token is stored anywhere: crates.io and npm accept uploads only
from those workflows, through trusted publishing (OIDC). To check a
download:

- **Release binaries.** Check the archive against the release's
  `SHA256SUMS.txt`:
  ```sh
  shasum -a 256 -c SHA256SUMS.txt --ignore-missing
  ```
  Then check that it was built by this repository's release workflow:
  ```sh
  gh attestation verify epubveri-<target>.tar.gz --repo veripublica/epubveri
  ```
- **npm** (`@veripublica/epubveri-wasm`) carries a provenance statement, and
  `npm audit signatures` checks it.
- **crates.io** is published by `publish-crate.yml` from the release tag.
