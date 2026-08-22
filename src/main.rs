//! Thin CLI for epubveri, following the **veripublica CLI convention v0.4**
//! (<https://github.com/veripublica/conventions>).
//!
//! epubveri is a *verifier*: it reads inputs and reports, writing no files and
//! asking no questions, so the convention's output-safety (`-o`/`-f`) and
//! prompt (`-y`) rules do not apply. What does apply: `-i/--input` as the only
//! input form (no positional path), the accepted argument syntaxes, loud
//! failure on anything unrecognized, the stream/exit-code rules, and the help
//! floor.
//!
//! The [`parse`] routine below is deliberately small, dependency-free and
//! commented: epubsana needs the *identical* argument grammar next and will
//! port it, so it is written to be read.
//!
//! Exit codes: `0` = every input valid (no errors), `1` = at least one input
//! has errors, `2` = usage error or an input could not be read.

use std::path::Path;
use std::process::ExitCode;

const HELP: &str = "\
epubveri — a pure-Rust EPUB validator

USAGE:
    epubveri -i <PATH> [OPTIONS]
    epubveri -i a.epub -i b.epub [OPTIONS]   validate several; report on each

OPTIONS:
    -i, --input <PATH>     The input. The only input form; positional paths are
                           not accepted. Repeat to validate several inputs.
        --format <FORMAT>  Report format: human (the default), json, or ids.
                           json is the shared machine envelope (one JSON object,
                           see the veripublica FORMATS spec).
        --profile <NAME>   Also check against an EPUB extension profile: one of
                           dict, edupub, idx, preview.
    -v, --epub-version <V> Validate against this EPUB version (2, 2.0, 3, 3.0)
                           whatever the book declares — epubcheck's -v. On a
                           disagreement PKG-001 says so and the requested
                           version wins, so a 3.0 book checked as 2.0 reports
                           at length. Default: the version the book declares.
                           (Note: -V, below, prints this tool's version.)
        --sort <ORDER>     Order the human report: severity (the default) or
                           document. severity groups the findings most-severe
                           first — fatal, error, warning, info — so the ones
                           that decide the verdict are together and first;
                           within each group the file order is unchanged, so
                           each group still reads top-to-bottom. document is
                           one pass in file order, whatever the severities.
                           **Affects the human report only**: --format json
                           and --format ids are always in document order, so a
                           tool never sees an order its user chose.
    -u, --usage            Show usage-severity findings, which name a feature a
                           book uses rather than anything wrong with it (an
                           @font-face declaration, an epub:type value outside
                           the default vocabulary, a manifest entry no document
                           references). Hidden by default, as in epubcheck.
                           Findings from --advisory print whichever way this is
                           set - that flag is their switch. This decides what
                           EVERY format contains, json and ids included, and the
                           json summary counts describe the output - as
                           epubcheck's -u does. The library API is never
                           filtered.
        --advisory         Also emit opt-in findings epubcheck has no verdict
                           on, at usage severity, in two families:
                             NEXT-*  a published spec requires it and epubcheck
                                     has not implemented it yet, so it becomes
                                     a real error once it does — today the
                                     EPUB 3.4 rules (page-spread-* on a
                                     reflowable document, roll-layout
                                     constraints, features deprecated in 3.4).
                             ADV-*   no spec says anything, but the book is
                                     still wrong — unknown CSS property or
                                     descriptor names, a type selector naming
                                     no known element, an EPUB 2 package
                                     written in EPUB 3, two NCX navigation
                                     entries landing on one document.
                           Off by default; neither ever affects the verdict or
                           the exit code.
    -V, --version          Print epubveri <version> to stdout and exit 0.
    -h, --help             Print this help to stdout and exit 0.

EXAMPLES:
    epubveri -i book.epub               # validate one book
    epubveri -i a.epub -i b.epub        # validate several; the exit code aggregates
    epubveri --format json -i book.epub # emit the machine envelope on stdout

EXIT CODES:
    0   every input is valid (no errors).
    1   every input was processed; at least one has errors.
    2   the tool could not run: a usage error, or an input that could not be read.

Conforms to veripublica conventions v0.4.";

/// Both spellings epubcheck accepts for each version, normalized to the bare
/// major so the library never has to parse a version string. `None` means the
/// value is not a version at all.
fn normalize_epub_version(value: &str) -> Option<String> {
    match value {
        "2" | "2.0" => Some("2".to_string()),
        "3" | "3.0" => Some("3".to_string()),
        _ => None,
    }
}

/// The outcome of parsing `argv` — decided entirely before any work is done.
#[derive(Debug, PartialEq)]
enum Cli {
    /// Validate every `inputs` entry, in command-line order.
    Run {
        inputs: Vec<String>,
        format: String,
        profile: Option<String>,
        /// The EPUB version to validate against ("2"/"3"), whatever the book
        /// declares - epubcheck's `-v`. `None` means "as declared".
        epub_version: Option<String>,
        advisory: bool,
        usage: bool,
        /// How the **human** report is ordered. Never reaches `json`/`ids`.
        sort: String,
    },
    /// `-h`/`--help` was requested (short-circuits everything else).
    Help,
    /// `-V`/`--version` was requested.
    Version,
    /// The invocation was malformed; the string is the short problem message
    /// (without the `error:` prefix or the `--help` pointer main adds).
    Usage(String),
}

/// Parse the arguments after the program name into a [`Cli`] decision.
///
/// The accepted syntaxes are the convention's (§3.3): `--name value` and
/// `--name=value`; `-i value` and the attached `-ivalue`; boolean short flags
/// bundle (`-hV`); a value-taking short flag consumes the rest of its token, or
/// the next token, as its value (POSIX: `-iv` means `-i v`); and the token
/// after a value-taking option is *always* its value, never re-parsed as an
/// option (`-i -q.epub` names the file `-q.epub`).
fn parse(args: &[String]) -> Cli {
    let mut inputs: Vec<String> = Vec::new();
    let mut format: Option<String> = None;
    let mut profile: Option<String> = None;
    let mut epub_version: Option<String> = None;
    let mut sort: Option<String> = None;
    let mut advisory = false;
    let mut usage = false;
    let mut help = false;
    let mut version = false;
    let mut error: Option<String> = None;

    // Record the first usage error but keep scanning, so a later `-h` can still
    // short-circuit a malformed line (§5). Help wins over any error below.
    macro_rules! fail {
        ($($a:tt)*) => {{ if error.is_none() { error = Some(format!($($a)*)); } }};
    }
    // Assign a value to a single-valued option, rejecting a second answer (§3.4).
    macro_rules! set_single {
        ($slot:expr_2021, $name:literal, $value:expr_2021) => {{
            if $slot.is_some() {
                fail!(concat!("option '", $name, "' given more than once"));
            } else {
                $slot = Some($value);
            }
        }};
    }

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--" {
            // Accepted and ignored; the convention gives it no other meaning.
        } else if let Some(long) = arg.strip_prefix("--") {
            let (name, attached) = match long.split_once('=') {
                Some((n, v)) => (n, Some(v.to_string())),
                None => (long, None),
            };
            match name {
                "help" => help = true,
                "version" => version = true,
                "advisory" => advisory = true,
                "usage" => usage = true,
                "input" | "format" | "profile" | "epub-version" | "sort" => {
                    let value = match attached {
                        Some(v) => v,
                        None => {
                            i += 1;
                            match args.get(i) {
                                Some(v) => v.clone(),
                                None => {
                                    fail!("option '--{name}' needs a value");
                                    break;
                                }
                            }
                        }
                    };
                    match name {
                        "input" => inputs.push(value),
                        "format" => set_single!(format, "--format", value),
                        "sort" => set_single!(sort, "--sort", value),
                        "profile" => set_single!(profile, "--profile", value),
                        "epub-version" => match normalize_epub_version(&value) {
                            Some(v) => set_single!(epub_version, "--epub-version", v),
                            None => fail!(
                                "invalid value '{value}' for --epub-version; \
                                 supported values: 2, 2.0, 3, 3.0"
                            ),
                        },
                        _ => unreachable!(),
                    }
                }
                _ => fail!("unexpected option '--{name}'"),
            }
        } else if arg.len() > 1 && arg.starts_with('-') {
            // A short cluster: booleans bundle; the first value-taking flag ends
            // it by consuming the remainder of the token (or the next token).
            let chars: Vec<char> = arg[1..].chars().collect();
            let mut j = 0;
            while j < chars.len() {
                match chars[j] {
                    'h' => help = true,
                    'u' => usage = true,
                    'V' => version = true,
                    // epubcheck spells this `-v`; `-V` is this tool's own
                    // version, as the convention requires. The two being one
                    // letter apart is inherited, not chosen - the help text
                    // spells out which is which.
                    'v' | 'i' => {
                        let flag = chars[j];
                        let rest: String = chars[j + 1..].iter().collect();
                        let value = if !rest.is_empty() {
                            rest
                        } else {
                            i += 1;
                            match args.get(i) {
                                Some(v) => v.clone(),
                                None => {
                                    fail!("option '-{flag}' needs a value");
                                    break;
                                }
                            }
                        };
                        if flag == 'i' {
                            inputs.push(value);
                        } else {
                            match normalize_epub_version(&value) {
                                Some(v) => set_single!(epub_version, "-v", v),
                                // Checked here rather than after the scan (as
                                // --format and --profile are) because of the
                                // collision this flag brings: `-v -i book.epub`
                                // feeds `-i` to `-v`, and the post-scan order
                                // would then blame the *book* for being a
                                // positional argument - telling the user to
                                // use `-i`, which is exactly what they did.
                                None => fail!(
                                    "invalid value '{value}' for -v; \
                                     supported values: 2, 2.0, 3, 3.0"
                                ),
                            }
                        }
                        break; // the value consumed the rest of the cluster
                    }
                    c => {
                        fail!("unexpected option '-{c}'");
                        break;
                    }
                }
                j += 1;
            }
        } else {
            // A bare word: positional inputs are not accepted (§2). Point the
            // user straight at the form that works.
            fail!("unexpected argument '{arg}'; use -i {arg}");
        }
        i += 1;
    }

    // Reject an out-of-set value for an enum option (§3.5) — after the scan, so
    // a `-h` anywhere still short-circuits to help rather than this error.
    if let Some(f) = &format
        && !["human", "json", "ids"].contains(&f.as_str())
    {
        fail!("invalid value '{f}' for --format; supported values: human, json, ids");
    }
    if let Some(o) = &sort
        && !["severity", "document"].contains(&o.as_str())
    {
        fail!("invalid value '{o}' for --sort; supported values: severity, document");
    }
    if let Some(p) = &profile
        && !epubveri::PROFILES.contains(&p.as_str())
    {
        fail!(
            "invalid value '{p}' for --profile; supported values: {}",
            epubveri::PROFILES.join(", ")
        );
    }

    // Precedence: help short-circuits even a malformed line; a usage error
    // outranks a version request; version outranks a run; a run needs an input.
    if help {
        return Cli::Help;
    }
    if let Some(msg) = error {
        return Cli::Usage(msg);
    }
    if version {
        return Cli::Version;
    }
    if inputs.is_empty() {
        return Cli::Usage("missing required -i".to_string());
    }
    Cli::Run {
        inputs,
        format: format.unwrap_or_else(|| "human".to_string()),
        profile,
        epub_version,
        advisory,
        usage,
        sort: sort.unwrap_or_else(|| "severity".to_string()),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse(&args) {
        Cli::Help => {
            println!("{HELP}");
            ExitCode::SUCCESS
        }
        Cli::Version => {
            println!("epubveri {}", epubveri::VERSION);
            ExitCode::SUCCESS
        }
        Cli::Usage(msg) => {
            // Short stderr message + a pointer to --help; never the full help.
            eprintln!("error: {msg} (see --help)");
            ExitCode::from(2)
        }
        Cli::Run {
            inputs,
            format,
            profile,
            epub_version,
            advisory,
            usage,
            sort,
        } => run(
            &inputs,
            &format,
            profile,
            epub_version,
            advisory,
            usage,
            &sort,
        ),
    }
}

/// Whether a report is the synthetic "this file does not exist" one - see the
/// PKG-018 branch in [`run`].
fn has_pkg_018(report: &epubveri::report::Report) -> bool {
    report
        .messages
        .iter()
        .any(|m| m.id == epubveri::ids::PKG_018)
}

/// Validate every input, report on each, and aggregate the exit code: `2` if
/// any input could not be read, else `1` if any has errors/fatals, else `0`.
fn run(
    inputs: &[String],
    format: &str,
    profile: Option<String>,
    epub_version: Option<String>,
    advisory: bool,
    usage: bool,
    sort: &str,
) -> ExitCode {
    let options = epubveri::Options {
        profile,
        epub_version,
        advisory,
    };
    // Validate everything first; an input that can't be read carries its own
    // message rather than a verdict.
    let results: Vec<(&String, Result<epubveri::report::Report, String>)> = inputs
        .iter()
        .map(|path| {
            let r = match epubveri::validate_path_with_options(Path::new(path), &options) {
                Ok(report) => Ok(report),
                // A path that simply isn't there is the one I/O failure with
                // an epubcheck message ID of its own, so it is reported as a
                // finding - which also carries it through the JSON envelope
                // like any other. Every other failure (a permission problem,
                // a directory, a broken symlink) has no ID and stays a plain
                // message. The exit code is unaffected either way: nothing
                // was validated, so it stays 2 (see the aggregation below).
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    let mut report = epubveri::report::Report::new();
                    report.push_at(
                        epubveri::ids::PKG_018,
                        epubveri::report::Severity::Fatal,
                        "the EPUB file could not be found",
                        path.as_str(),
                    );
                    Ok(report)
                }
                // A directory is the one wrong input a person arrives at on
                // purpose rather than by typo: epubcheck validates an
                // unpacked EPUB with `-mode exp`, so someone porting an
                // invocation reasonably tries it here. **We take the packaged
                // file only, by decision** — the file is the unit the
                // veripublica tools hand to each other — so the message says
                // that outright instead of surfacing an OS error the reader
                // has to interpret.
                Err(e) if e.kind() == std::io::ErrorKind::IsADirectory => Err(format!(
                    "{path} is a directory. epubveri validates the packaged \
                     .epub file; unpacked EPUB directories are not supported. \
                     Zip the directory first (mimetype first and stored), or \
                     pass the .epub file."
                )),
                Err(e) => Err(format!("cannot read {path}: {e}")),
            };
            (path, r)
        })
        .collect();

    let mut worst: u8 = 0;
    for (_, r) in &results {
        worst = worst.max(match r {
            Ok(report) if report.is_valid() => 0,
            // PKG-018 is a finding, but it is still "the input could not be
            // read" - no verdict was possible, so it exits 2 like any other
            // unreadable input rather than 1 (§6).
            Ok(report) if has_pkg_018(report) => 2,
            Ok(_) => 1,
            Err(_) => 2, // no verdict was possible (§6)
        });
    }

    // **The output view, built once and used by every format.** The verdict and
    // the exit code were decided above from the *unfiltered* reports, so this
    // cannot move them — and usage findings never counted toward a verdict
    // anyway. Filtering here rather than inside each format is what keeps the
    // flag's meaning single: `-u` includes usage-severity findings in the
    // output, whichever format is being written.
    //
    // Doitsu (MobileRead #231) asked for this after 0.10.0 shipped with the
    // filter on the human report alone, and he was right: epubcheck's own `-u`
    // gates its JSON too, counts included, so a command line ported between the
    // two tools returned different data. The library is a different question
    // and keeps a different answer — `validate_bytes` never filters, because
    // epubsana dispatches repairs on findings below error severity.
    let results: Vec<(&String, Result<epubveri::report::Report, String>)> = results
        .into_iter()
        .map(|(path, r)| {
            (
                path,
                r.map(|report| {
                    if usage {
                        return report;
                    }
                    let mut out = report.clone();
                    out.messages.retain(|m| shown_to_a_reader(m, usage));
                    out
                }),
            )
        })
        .collect();

    if format == "json" {
        // One JSON object on stdout; an unreadable input is described *inside*
        // it (status "error"), not on stderr.
        let envelope = epubveri::envelope::Envelope::new(
            results
                .into_iter()
                .map(|(path, r)| match r {
                    Ok(report) => epubveri::envelope::Input::from_report(path.clone(), &report),
                    Err(e) => epubveri::envelope::Input::from_error(path.clone(), e),
                })
                .collect(),
        );
        println!("{}", serde_json::to_string_pretty(&envelope).unwrap());
    } else {
        let multi = results.len() > 1;
        for (path, r) in &results {
            match r {
                Ok(report) => print_report(report, path, format, multi, sort),
                Err(e) => eprintln!("error: {e}"),
            }
        }
    }

    match worst {
        0 => ExitCode::SUCCESS,
        n => ExitCode::from(n),
    }
}

/// Print one input's report to stdout in the requested `human`/`ids` format.
/// With multiple inputs, a `human` report is preceded by a path header so each
/// verdict is attributable.
/// Is this finding shown in the human rendering?
///
/// **Only usage findings are ever hidden, and only from this rendering.**
/// epubcheck's default level is fatal/error/warning/info with usage excluded
/// unless `-u` — read off its own `--help`, not assumed — so `info` stays
/// visible here too.
///
/// **The exception is the category epubcheck does not have.** `ADV-*`/`NEXT-*`
/// findings are emitted at usage severity, so filtering on severity alone
/// would make `--advisory` print nothing at all: the flag would go quietly
/// inert, which is the failure mode this project keeps having to undo. They
/// are shown whenever they are present, because `--advisory` has already
/// decided that — the library does not emit them otherwise.
fn shown_to_a_reader(m: &epubveri::report::Message, usage: bool) -> bool {
    m.severity != epubveri::report::Severity::Usage
        || usage
        || epubveri::ids::advisory_basis(m.id).is_some()
}

fn print_report(
    report: &epubveri::report::Report,
    path: &str,
    format: &str,
    multi: bool,
    sort: &str,
) {
    // `report` is already the output view — `run` applied the `-u` filter once,
    // for every format. Nothing is filtered a second time here.
    if format == "ids" {
        for m in &report.messages {
            println!("{}", m.id);
        }
        return;
    }
    if multi {
        println!("=== {path} ===");
    }
    // The line format lives in the library, not here, so a consumer embedding
    // epubveri prints byte-identical findings instead of reimplementing this
    // and drifting from it (epubsana's request, 2026-08-21). The multi-input
    // header above stays CLI-only: it is about this program's arguments, not
    // about a report.
    //
    // Rendered message by message rather than through `Report::render_human`
    // so the filter can sit between them: that method is public API and its
    // contract is the whole report. `render_summary` is the same primitive it
    // uses, so the verdict line is unchanged — and the verdict itself cannot
    // move, since usage findings never counted toward it.
    let mut shown: Vec<&epubveri::report::Message> = report.messages.iter().collect();
    // **A stable sort on severity alone, deliberately.** The library already
    // hands us document order, so sorting by nothing but the severity rank
    // leaves each group internally in document order - one pass down the
    // errors, then one down the warnings, each still reading top-to-bottom.
    // Recomputing a `(severity, file, line, column)` key would produce the
    // same answer and would silently stop matching the library's file
    // ordering the day that changes.
    //
    // `Severity` is declared most-severe-first, so `as u8` is the rank; there
    // is no separate table to keep in step with it.
    if sort == "severity" {
        shown.sort_by_key(|m| m.severity as u8);
    }
    for m in shown {
        println!("{}", m.render_human());
    }
    println!("{}", report.render_summary());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `--sort severity` groups most-severe-first **and leaves the order
    /// inside each group alone**.
    ///
    /// That second half is the whole design: the library hands the CLI document
    /// order, and a *stable* sort on the severity rank alone preserves it
    /// within each group — so the reader gets one pass down the errors and then
    /// one down the warnings, each still reading top-to-bottom. A sort that
    /// recomputed a `(severity, file, line, column)` key would pass a naive
    /// test and quietly stop agreeing with the library's file ordering the day
    /// that changed, so the fixture below is deliberately **not** in file-name
    /// order: `b.xhtml` precedes `a.xhtml`, and a comparator that re-derived
    /// the order would put them the other way round.
    #[test]
    fn severity_sort_groups_by_rank_and_keeps_document_order_inside_each_group() {
        use epubveri::report::{Position, Report, Severity};

        let mut r = Report::new();
        let mut add = |sev: Severity, file: &str, line: u32| {
            r.push_at_pos(
                epubveri::ids::RSC_005,
                sev,
                format!("{file}:{line}"),
                file,
                Position { line, column: 1 },
            );
        };
        // Interleaved, in the order the library would emit them.
        add(Severity::Warning, "b.xhtml", 1);
        add(Severity::Error, "b.xhtml", 2);
        add(Severity::Warning, "a.xhtml", 1);
        add(Severity::Fatal, "a.xhtml", 2);
        add(Severity::Error, "a.xhtml", 3);

        let ordered = |sort: &str| -> Vec<String> {
            let mut shown: Vec<&epubveri::report::Message> = r.messages.iter().collect();
            if sort == "severity" {
                shown.sort_by_key(|m| m.severity as u8);
            }
            shown
                .iter()
                .map(|m| format!("{} {}", m.severity, m.text))
                .collect()
        };

        assert_eq!(
            ordered("severity"),
            vec![
                "FATAL a.xhtml:2",
                "ERROR b.xhtml:2",
                "ERROR a.xhtml:3",
                "WARNING b.xhtml:1",
                "WARNING a.xhtml:1",
            ],
            "most severe first, and each group in the order it arrived"
        );
        assert_eq!(
            ordered("document"),
            vec![
                "WARNING b.xhtml:1",
                "ERROR b.xhtml:2",
                "WARNING a.xhtml:1",
                "FATAL a.xhtml:2",
                "ERROR a.xhtml:3",
            ],
            "document order is what the library handed us, untouched"
        );
    }

    /// The order is a **display** choice and must not reach a machine
    /// consumer: `--format json` and `--format ids` are always document order,
    /// whatever the user typed.
    ///
    /// Asserted on the parse rather than the output because that is where it
    /// could go wrong — `print_report` returns before the sort for `ids`, and
    /// the json path never sees `sort` at all. A future edit that threads it
    /// into either would make a tool's input depend on its user's preference.
    #[test]
    fn sort_is_accepted_everywhere_but_only_the_human_format_may_use_it() {
        let cli = parse(&[
            "--sort".into(),
            "document".into(),
            "--format".into(),
            "json".into(),
            "-i".into(),
            "b.epub".into(),
        ]);
        match cli {
            Cli::Run { sort, format, .. } => {
                assert_eq!(sort, "document");
                assert_eq!(format, "json");
            }
            other => panic!("expected a run, got {other:?}"),
        }
        // The source of truth for "json ignores it" is print_report's shape:
        // the ids branch returns before the sort, and neither machine branch
        // reads `sort`.
        assert!(
            !include_str!("main.rs")
                .split("if format == \"ids\"")
                .nth(1)
                .unwrap_or("")
                .split("if multi")
                .next()
                .unwrap_or("")
                .contains("sort"),
            "the ids branch must not consult --sort"
        );
    }

    /// An out-of-set value is refused with the supported values named, like
    /// `--format` and `--profile`.
    #[test]
    fn an_unknown_sort_order_is_a_usage_error() {
        match parse(&[
            "--sort".into(),
            "sideways".into(),
            "-i".into(),
            "b.epub".into(),
        ]) {
            Cli::Usage(msg) => {
                assert!(msg.contains("--sort"), "got {msg}");
                assert!(msg.contains("severity, document"), "got {msg}");
            }
            other => panic!("expected a usage error, got {other:?}"),
        }
    }

    /// The default is `severity`, and it is the default *in the parse* rather
    /// than a fallback further down — so every caller of `Cli::Run` sees the
    /// same answer.
    #[test]
    fn the_default_sort_is_severity() {
        match parse(&["-i".into(), "b.epub".into()]) {
            Cli::Run { sort, .. } => assert_eq!(sort, "severity"),
            other => panic!("expected a run, got {other:?}"),
        }
    }

    /// `-u` decides what **every** format contains, not only the human report.
    ///
    /// 0.10.0 shipped it as a display filter, on the reasoning that a machine
    /// consumer receiving fewer findings than the library produced cannot
    /// recover what it never got. Doitsu — who wrote the Sigil plugin — reported
    /// the inconsistency within hours (MobileRead #231), and measuring settled
    /// it: **epubcheck's own `-u` gates its JSON too, counts included**
    /// (`nUsage` drops to 0 without the flag). A command line ported between
    /// the two tools was returning different data, and one flag meaning two
    /// things depending on `--format` is not a contract anyone should have to
    /// remember.
    ///
    /// The concern that motivated the original choice is answered elsewhere and
    /// better: **the library never filters**, whatever flags the CLI was given,
    /// which is what actually protects a consumer like epubsana that dispatches
    /// repairs on findings below error severity. That boundary has its own
    /// test.
    #[test]
    fn the_usage_filter_reaches_every_format_and_the_counts_follow() {
        use epubveri::report::{Report, Severity};

        let mut r = Report::new();
        r.push(epubveri::ids::CSS_028, Severity::Usage, "a feature note");
        r.push(epubveri::ids::RSC_005, Severity::Error, "a real problem");
        r.push(epubveri::ids::RSC_004, Severity::Info, "a neutral fact");

        let view = |usage: bool| -> Report {
            let mut out = r.clone();
            out.messages.retain(|m| shown_to_a_reader(m, usage));
            out
        };

        // Without -u: the usage finding is gone from the report every format is
        // written from, and `info` is not — epubcheck's default level keeps it.
        let hidden = view(false);
        assert_eq!(
            hidden.messages.iter().map(|m| m.id).collect::<Vec<_>>(),
            vec![epubveri::ids::RSC_005, epubveri::ids::RSC_004]
        );
        // The counts describe the output, as epubcheck's nUsage does.
        assert_eq!(hidden.usages(), 0);
        assert_eq!(hidden.infos(), 1);

        let shown = view(true);
        assert_eq!(shown.messages.len(), 3);
        assert_eq!(shown.usages(), 1);

        // The verdict cannot move either way: usage never counted toward it.
        assert_eq!(hidden.errors(), shown.errors());
        assert_eq!(hidden.is_valid(), shown.is_valid());
    }

    /// Usage findings are hidden by default — **except the family that shares
    /// their severity but not their meaning**.
    ///
    /// `ADV-*`/`NEXT-*` are emitted at usage severity, so a filter written as
    /// `severity != Usage` would make `--advisory` print nothing at all. The
    /// flag would not fail; it would go quietly inert, and nothing on either
    /// side would report it. That is the assertion worth having here — the
    /// plain usage case is the easy half.
    ///
    /// The verdict cannot move either way: usage findings never counted toward
    /// it, which is why this can be a display filter at all.
    #[test]
    fn only_usage_hides_and_the_advisory_family_is_exempt() {
        use epubveri::report::{Report, Severity};

        let mut r = Report::new();
        r.push(epubveri::ids::CSS_028, Severity::Usage, "a feature note");
        r.push(epubveri::ids::ADV_003, Severity::Usage, "an advisory");
        r.push(
            epubveri::ids::NEXT_005,
            Severity::Usage,
            "a spec-ahead advisory",
        );
        r.push(epubveri::ids::RSC_005, Severity::Error, "an error");
        r.push(epubveri::ids::CSS_007, Severity::Info, "an info");

        let visible = |usage: bool| -> Vec<&'static str> {
            r.messages
                .iter()
                .filter(|m| shown_to_a_reader(m, usage))
                .map(|m| m.id)
                .collect()
        };

        assert_eq!(
            visible(false),
            vec![
                epubveri::ids::ADV_003,
                epubveri::ids::NEXT_005,
                epubveri::ids::RSC_005,
                epubveri::ids::CSS_007,
            ],
            "by default: usage hidden, advisory kept, info kept as epubcheck keeps it"
        );
        assert_eq!(
            visible(true),
            vec![
                epubveri::ids::CSS_028,
                epubveri::ids::ADV_003,
                epubveri::ids::NEXT_005,
                epubveri::ids::RSC_005,
                epubveri::ids::CSS_007,
            ],
            "-u shows everything"
        );
    }

    /// A tripwire rather than a behaviour test: `--advisory`'s help text
    /// enumerates what the flag emits, in prose, and **nothing links that
    /// prose to `ids.rs`**.
    ///
    /// It has already gone stale once. Through 0.9.13/0.9.14 the paragraph
    /// still described only the CSS lint that shipped in 0.9.0, while the
    /// flag had grown a type-selector lint, the EPUB-2-package advisory and
    /// four EPUB 3.4 rules — none of which a reader of `--help` could learn
    /// existed. 0.9.25 rewrote it by hand, and ADV-009 nearly recreated the
    /// same gap a day later.
    ///
    /// The failure mode is why this is worth a test: a stale help text breaks
    /// no check, changes no verdict, and **no instrument here reads it** —
    /// not the corpus, not the shelf, not `compare`. It is only ever caught
    /// by a person noticing, which is how it survived five releases.
    ///
    /// So when this fails, an advisory check was added or removed. Describe it
    /// in `HELP`'s `--advisory` paragraph, then update the count below.
    ///
    /// Counts **both** families: the flag emits `NEXT-*` and `ADV-*`, and a
    /// new check of either kind needs describing. Watching only one would have
    /// let four EPUB 3.4 rules move families unremarked.
    #[test]
    fn a_new_advisory_check_must_be_described_in_the_help_text() {
        let declared: Vec<&str> = include_str!("ids.rs")
            .lines()
            .filter_map(|l| {
                let t = l.trim();
                t.strip_prefix("pub const ADV_")
                    .or_else(|| t.strip_prefix("pub const NEXT_"))
            })
            .filter_map(|l| l.split('"').nth(1))
            .collect();
        assert_eq!(
            declared.len(),
            9,
            "the advisory families changed ({declared:?}) — describe the new \
             check in --advisory's help text, then update this count"
        );
        // The count above means nothing if the paragraph it guards has gone,
        // and both families have to stay named in it.
        // Split on the *option entry*, not the first mention of the flag. It
        // used to be `split("--advisory")`, which broke the moment another
        // entry referred to the flag in prose — `-u`'s does, since the two
        // interact — and the failure looked like the advisory paragraph had
        // gone missing. The definition line is the thing being guarded, so
        // match its exact shape.
        let advisory_help = HELP
            .split("\n        --advisory ")
            .nth(1)
            .expect("HELP documents the --advisory option entry");
        assert!(
            advisory_help.contains("ADV-*")
                && advisory_help.contains("NEXT-*")
                && advisory_help.len() > 200,
            "the --advisory paragraph should still describe both families"
        );
    }

    fn parse_str(argv: &[&str]) -> Cli {
        parse(&argv.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    fn run_of(argv: &[&str]) -> (Vec<String>, String, Option<String>, bool) {
        match parse_str(argv) {
            Cli::Run {
                inputs,
                format,
                profile,
                advisory,
                ..
            } => (inputs, format, profile, advisory),
            other => panic!("expected Run, got {other:?}"),
        }
    }

    #[test]
    fn bare_invocation_is_missing_input_not_help() {
        assert_eq!(parse_str(&[]), Cli::Usage("missing required -i".into()));
    }

    #[test]
    fn positional_is_rejected_with_a_migration_hint() {
        assert_eq!(
            parse_str(&["book.epub"]),
            Cli::Usage("unexpected argument 'book.epub'; use -i book.epub".into())
        );
    }

    #[test]
    fn input_forms_all_name_the_same_file() {
        for argv in [
            vec!["-i", "book.epub"],
            vec!["--input", "book.epub"],
            vec!["--input=book.epub"],
            vec!["-ibook.epub"],
        ] {
            let (inputs, format, profile, advisory) = run_of(&argv);
            assert_eq!(inputs, vec!["book.epub"]);
            assert_eq!(format, "human");
            assert_eq!(profile, None);
            assert!(!advisory, "advisory must default off");
        }
    }

    #[test]
    fn repeated_input_accumulates_in_order() {
        let (inputs, _, _, _) = run_of(&["-i", "a.epub", "-i", "b.epub"]);
        assert_eq!(inputs, vec!["a.epub", "b.epub"]);
    }

    #[test]
    fn a_value_token_is_never_reparsed_as_an_option() {
        // The token after -i is its value even when it looks like a flag.
        let (inputs, _, _, _) = run_of(&["-i", "-q.epub"]);
        assert_eq!(inputs, vec!["-q.epub"]);
    }

    #[test]
    fn bundled_value_flag_takes_the_remainder_posix() {
        // -iv means -i v, not -i -v.
        let (inputs, _, _, _) = run_of(&["-iv"]);
        assert_eq!(inputs, vec!["v"]);
    }

    #[test]
    fn repeated_single_valued_option_is_an_error() {
        assert_eq!(
            parse_str(&["-i", "a.epub", "--format", "human", "--format", "ids"]),
            Cli::Usage("option '--format' given more than once".into())
        );
    }

    #[test]
    fn unknown_option_is_a_usage_error() {
        // An unrecognized flag is named, not swallowed as a path. (This used
        // to be spelled with `-v`, which is now the EPUB-version flag.)
        assert_eq!(
            parse_str(&["-z", "-i", "a.epub"]),
            Cli::Usage("unexpected option '-z'".into())
        );
        assert_eq!(
            parse_str(&["--bogus"]),
            Cli::Usage("unexpected option '--bogus'".into())
        );
    }

    /// `-v`/`--epub-version` (#61), normalized to the bare major so the
    /// library never parses a version string.
    #[test]
    fn epub_version_flag_normalizes_both_spellings() {
        let version_of = |argv: &[&str]| match parse_str(argv) {
            Cli::Run { epub_version, .. } => epub_version,
            other => panic!("expected Run, got {other:?}"),
        };
        assert_eq!(version_of(&["-i", "a.epub"]), None);
        assert_eq!(version_of(&["-i", "a.epub", "-v", "2"]), Some("2".into()));
        assert_eq!(version_of(&["-i", "a.epub", "-v", "2.0"]), Some("2".into()));
        assert_eq!(version_of(&["-i", "a.epub", "-v3.0"]), Some("3".into()));
        assert_eq!(
            version_of(&["-i", "a.epub", "--epub-version=3"]),
            Some("3".into())
        );
        assert_eq!(
            parse_str(&["-i", "a.epub", "-v", "4"]),
            Cli::Usage("invalid value '4' for -v; supported values: 2, 2.0, 3, 3.0".into())
        );
        assert_eq!(
            parse_str(&["-i", "a.epub", "-v", "2", "-v", "3"]),
            Cli::Usage("option '-v' given more than once".into())
        );
    }

    /// The trap this flag brings with it, recorded on purpose: the token after
    /// a value-taking option is *always* its value (§3.3), so `-v -i x.epub`
    /// asks for EPUB version "-i" rather than quietly doing something
    /// sensible. Being one letter from `-V` makes the slip easy, so it must
    /// fail loudly rather than validate the wrong thing.
    #[test]
    fn epub_version_consumes_the_next_token_even_if_it_looks_like_a_flag() {
        assert_eq!(
            parse_str(&["-v", "-i", "a.epub"]),
            Cli::Usage("invalid value '-i' for -v; supported values: 2, 2.0, 3, 3.0".into())
        );
    }

    #[test]
    fn unknown_enum_values_are_rejected() {
        assert_eq!(
            parse_str(&["-i", "a.epub", "--format", "xml"]),
            Cli::Usage(
                "invalid value 'xml' for --format; supported values: human, json, ids".into()
            )
        );
        assert!(matches!(
            parse_str(&["-i", "a.epub", "--profile", "nope"]),
            Cli::Usage(_)
        ));
    }

    #[test]
    fn json_is_an_accepted_format() {
        let (_, format, _, _) = run_of(&["--format", "json", "-i", "a.epub"]);
        assert_eq!(format, "json");
    }

    #[test]
    fn help_short_circuits_even_a_malformed_line() {
        assert_eq!(parse_str(&["--bogus", "-h"]), Cli::Help);
        assert_eq!(parse_str(&["-h"]), Cli::Help);
        // Help wins over version, and over a bundle carrying both.
        assert_eq!(parse_str(&["-hV"]), Cli::Help);
    }

    #[test]
    fn version_is_recognized_and_needs_no_input() {
        assert_eq!(parse_str(&["-V"]), Cli::Version);
        assert_eq!(parse_str(&["--version"]), Cli::Version);
    }

    #[test]
    fn profile_and_format_pass_through_when_valid() {
        let (inputs, format, profile, _) =
            run_of(&["--profile", "edupub", "--format", "ids", "-i", "a.epub"]);
        assert_eq!(inputs, vec!["a.epub"]);
        assert_eq!(format, "ids");
        assert_eq!(profile, Some("edupub".to_string()));
    }

    #[test]
    fn advisory_flag_is_parsed_and_defaults_off() {
        let (_, _, _, advisory) = run_of(&["-i", "a.epub"]);
        assert!(!advisory);
        let (_, _, _, advisory) = run_of(&["--advisory", "-i", "a.epub"]);
        assert!(advisory);
    }
}
