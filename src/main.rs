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
        --advisory         Also emit opt-in advisory findings epubcheck has no
                           verdict on, as ADV-* at usage severity: unknown CSS
                           property/descriptor names, a CSS type selector
                           naming no known element, an EPUB 2 package written
                           in EPUB 3, the restrictive EPUB 3.4 rules
                           (page-spread-* on a reflowable document, roll-layout
                           constraints, features deprecated in 3.4), and two
                           NCX navigation entries landing on one document. Off
                           by default; never affects the exit code.
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
    let mut advisory = false;
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
                "input" | "format" | "profile" | "epub-version" => {
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
        } => run(&inputs, &format, profile, epub_version, advisory),
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
                Ok(report) => print_report(report, path, format, multi),
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
fn print_report(report: &epubveri::report::Report, path: &str, format: &str, multi: bool) {
    if format == "ids" {
        for m in &report.messages {
            println!("{}", m.id);
        }
        return;
    }
    if multi {
        println!("=== {path} ===");
    }
    for m in &report.messages {
        let loc = m
            .location
            .as_deref()
            .map(|l| match m.position {
                Some(p) => format!(" [{l}:{}:{}]", p.line, p.column),
                None => format!(" [{l}]"),
            })
            .unwrap_or_default();
        println!("{} {}: {}{}", m.severity, m.id, m.text, loc);
    }
    // Lead with the fatal count only when there is one, so a fatal-only book
    // does not read as "0 error(s) … INVALID".
    let fatals = report.fatals();
    let fatal_head = if fatals > 0 {
        format!("{fatals} fatal, ")
    } else {
        String::new()
    };
    println!(
        "— {}{} error(s), {} warning(s): {}",
        fatal_head,
        report.errors(),
        report.warnings(),
        if report.is_valid() {
            "VALID"
        } else {
            "INVALID"
        }
    );
}

#[cfg(test)]
mod tests {
    use super::*;

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
