//! Measure epubveri against epubcheck's own test corpus (Cucumber
//! features). Rust port of the former `scripts/corpus.py`, being ported
//! incrementally and verified against the Python original at each step.
//!
//! Phase 1 (done): `.feature` file discovery + Gherkin parsing. Run with
//! `--dump` to print one canonical line per parsed scenario, for diffing
//! against the Python parser's own `--dump` mode.
//!
//! Phase 2 (current): the `zip_dir`/`wrap_*` fixture builders, in
//! `wrap.rs`. Run with `--wrap-test <kind> <full-path> <name> [version]`
//! to invoke one builder directly and print the resulting temp `.epub`
//! path, for diffing its extracted contents against the Python
//! original's.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;

mod wrap;

#[derive(Debug, Clone, Default)]
struct Scenario {
    file: PathBuf,
    base: Option<String>,
    name: Option<String>,
    errs: BTreeSet<String>,
    warns: BTreeSet<String>,
    clean: bool,
    as_nav: bool,
    edupub_profile: bool,
    idx_profile: bool,
    cli_profile: Option<String>,
}

fn id_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // A trailing lowercase letter (e.g. "HTM-060a"/"HTM-060b") is a
    // Gherkin-authoring convention to label sub-cases of the same real
    // epubcheck code, not part of the reported message id - matched but
    // not captured, so "HTM-060a" scores as "HTM-060".
    RE.get_or_init(|| Regex::new(r"\b([A-Z]{2,4}-\d{2,4})[a-z]?\b").unwrap())
}

fn check_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"checking (?:EPUB|document|file|the EPUB)\s+'([^']+)'").unwrap())
}

fn located_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"located at\s+'([^']+)'").unwrap())
}

fn cli_profile_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"configured with the '(\w+)' profile").unwrap())
}

fn following_errs_warns_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"the following (errors?|warnings?) are reported").unwrap())
}

fn no_errors_warnings_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"no (other )?errors? (or|and) warnings? (are|is) reported").unwrap()
    })
}

/// Recursively collect every `*.feature` file under `dir` (a hand-rolled
/// walk, not the `walkdir` crate - the corpus's directory tree is shallow
/// enough that this is simpler than a new dependency).
fn find_feature_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            find_feature_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "feature") {
            out.push(path);
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum TableMode {
    Err,
    Warn,
}

fn parse_feature_file(path: &Path, scenarios: &mut Vec<Scenario>) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    let mut base: Option<String> = None;
    let mut edupub_profile_bg = false;
    let mut idx_profile_bg = false;
    let mut cli_profile_bg: Option<String> = None;
    let mut cur: Option<usize> = None; // index into `scenarios` of the current scenario
    let mut table_mode: Option<TableMode> = None;

    for raw in text.lines() {
        let line = raw.trim();
        // A Gherkin comment line (e.g. a disabled assertion like
        // "#Then error RSC-005 is reported", left in on purpose by
        // epubcheck itself with a FIXME above it) must not be read as a
        // real assertion - without this, a commented-out expectation
        // silently corrupts scoring (a fixture we're already correctly
        // silent on looks like a miss).
        if line.starts_with('#') {
            continue;
        }
        if let Some(m) = located_re().captures(line) {
            base = Some(m[1].to_string());
        }
        // Declared either once in a feature file's Background (read
        // before `cur` exists at all for the first scenario - tracked as
        // a rolling flag, like `base`, and copied onto each new scenario
        // at creation time) or per-scenario inside an individual
        // scenario's own body (as `indexes-publication.feature` does for
        // only 3 of its 6 scenarios) - in that case it must set *only*
        // the current scenario, not leak into every scenario after it
        // the way a naive sticky flag would.
        if line.contains("EPUBCheck configured with the 'edupub' profile") {
            match cur {
                None => edupub_profile_bg = true,
                Some(i) => scenarios[i].edupub_profile = true,
            }
        }
        if line.contains("EPUBCheck configured with the 'idx' profile") {
            match cur {
                None => idx_profile_bg = true,
                Some(i) => scenarios[i].idx_profile = true,
            }
        }
        // A real `epubveri --profile <name>` value (any name, not just
        // edupub/idx) - passed straight to our binary so its own
        // profile-gating checks (dc:type-required, etc.) actually run,
        // on top of (not instead of) the wrap-synthesis mechanism above,
        // which still matters for content-document-level detection with
        // no package in play at all.
        if let Some(m) = cli_profile_re().captures(line) {
            let name = m[1].to_string();
            match cur {
                None => cli_profile_bg = Some(name),
                Some(i) => scenarios[i].cli_profile = Some(name),
            }
        }
        if line.starts_with("Scenario Outline") {
            cur = None; // skip parameterized outlines
            table_mode = None;
            continue;
        }
        // "Example:" is a real Gherkin synonym for "Scenario:" (used e.g.
        // in vocabularies.feature) - matching only the exact singular
        // keyword, since "Examples:" (plural) is the unrelated Scenario
        // Outline parameter-table keyword. Two feature files use it;
        // missing it let assertions bleed across scenario boundaries
        // (`cur` never reset), silently corrupting scoring for every
        // scenario after the first.
        if line.starts_with("Scenario") || line.starts_with("Example:") {
            scenarios.push(Scenario {
                file: path.to_path_buf(),
                base: base.clone(),
                name: None,
                errs: BTreeSet::new(),
                warns: BTreeSet::new(),
                clean: false,
                as_nav: false,
                edupub_profile: edupub_profile_bg,
                idx_profile: idx_profile_bg,
                cli_profile: cli_profile_bg.clone(),
            });
            cur = Some(scenarios.len() - 1);
            table_mode = None;
            continue;
        }
        let Some(cur_idx) = cur else { continue };
        if line.contains("EPUBCheck configured to check a navigation document") {
            scenarios[cur_idx].as_nav = true;
        }
        if let Some(m) = check_re().captures(line) {
            scenarios[cur_idx].name = Some(m[1].to_string());
        }
        // Cucumber table form: "And the following errors/warnings are
        // reported" followed by "| ID | message |" rows - these rows
        // don't repeat the phrase "is reported", so they need separate
        // handling, or scenarios using this form get misparsed as having
        // no expected errors (and can look like false clean-scenario
        // positives once we start reporting real ones).
        if let Some(m) = following_errs_warns_re().captures(line) {
            table_mode = Some(if m[1].starts_with("warning") {
                TableMode::Warn
            } else {
                TableMode::Err
            });
            continue;
        }
        if line.starts_with('|') {
            let ids: Vec<String> = id_re()
                .captures_iter(line)
                .map(|c| c[1].to_string())
                .collect();
            if !ids.is_empty() {
                let target = if table_mode == Some(TableMode::Warn) {
                    &mut scenarios[cur_idx].warns
                } else {
                    &mut scenarios[cur_idx].errs
                };
                target.extend(ids);
            }
            continue;
        }
        table_mode = None;
        // "X is reported 0 times" is a negative assertion (the ID must
        // NOT appear) - the opposite of every other "is reported"
        // phrasing here. Only 2 scenarios in the whole corpus use it;
        // without this check, both were misread as *expecting* the named
        // ID, backwards from their real (and, since they're paired with
        // "no other errors/warnings", fully clean) intent.
        if line.contains("is reported") && !line.contains("reported 0 times") {
            let ids: Vec<String> = id_re()
                .captures_iter(line)
                .map(|c| c[1].to_string())
                .collect();
            if line.contains("warning") {
                scenarios[cur_idx].warns.extend(ids);
            } else {
                // 'error' or 'fatal error'
                scenarios[cur_idx].errs.extend(ids);
            }
        }
        if no_errors_warnings_re().is_match(line) {
            scenarios[cur_idx].clean = true;
        }
    }
}

fn parse_features(res_dir: &Path) -> Vec<Scenario> {
    let mut files = Vec::new();
    find_feature_files(res_dir, &mut files);
    let mut scenarios = Vec::new();
    for f in &files {
        parse_feature_file(f, &mut scenarios);
    }
    scenarios.retain(|s| s.name.is_some());
    scenarios
}

fn dump_line(s: &Scenario, res_dir: &Path) -> String {
    let rel = s.file.strip_prefix(res_dir).unwrap_or(&s.file);
    let errs: Vec<&str> = s.errs.iter().map(|x| x.as_str()).collect();
    let warns: Vec<&str> = s.warns.iter().map(|x| x.as_str()).collect();
    format!(
        "{}\t{}\t{}\terrs={}\twarns={}\tclean={}\tas_nav={}\tedupub={}\tidx={}\tprofile={}",
        rel.display(),
        s.base.as_deref().unwrap_or(""),
        s.name.as_deref().unwrap_or(""),
        errs.join(","),
        warns.join(","),
        s.clean,
        s.as_nav,
        s.edupub_profile,
        s.idx_profile,
        s.cli_profile.as_deref().unwrap_or(""),
    )
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let res_dir = root
        .join("corpus")
        .join("epubcheck")
        .join("src")
        .join("test")
        .join("resources");
    if !res_dir.is_dir() {
        eprintln!("corpus not found at {}", res_dir.display());
        std::process::exit(1);
    }

    let args: Vec<String> = std::env::args().collect();

    // Debug-only mode for phase-2 verification: invoke one wrap builder
    // directly and print the resulting temp .epub path, so its extracted
    // contents can be diffed against the Python original's for the same
    // input fixture.
    if let Some(pos) = args.iter().position(|a| a == "--wrap-test") {
        let kind = &args[pos + 1];
        let full = PathBuf::from(&args[pos + 2]);
        let name = &args[pos + 3];
        let version = args.get(pos + 4).map(String::as_str).unwrap_or("3.0");
        let tmp = match kind.as_str() {
            "opf" => wrap::wrap_opf_file(&full, name),
            "single_doc" => wrap::wrap_single_doc(&full, name, version, false, false),
            "single_doc_edupub" => wrap::wrap_single_doc(&full, name, version, true, false),
            "single_doc_idx" => wrap::wrap_single_doc(&full, name, version, false, true),
            "nav_doc" => wrap::wrap_nav_doc(&full, name, version),
            "svg" => wrap::wrap_svg_file(&full, name),
            "smil" => wrap::wrap_smil_file(&full, name),
            "dir" => wrap::zip_dir(&full),
            other => {
                eprintln!("unknown --wrap-test kind: {other}");
                std::process::exit(1);
            }
        };
        println!("{}", tmp.display());
        return;
    }

    let scenarios = parse_features(&res_dir);

    if args.iter().any(|a| a == "--dump") {
        let mut lines: Vec<String> = scenarios.iter().map(|s| dump_line(s, &res_dir)).collect();
        lines.sort();
        for line in lines {
            println!("{line}");
        }
        return;
    }

    run_report(&scenarios, &res_dir);
}

enum Resolved {
    Path {
        path: PathBuf,
        is_temp: bool,
        single_doc_wrap: bool,
    },
    Skip(&'static str),
}

/// Faithful port of `resolve()`: dispatches a scenario to the matching
/// wrap builder (or a real file/directory), based purely on the
/// scenario's own resolved fixture name/extension.
fn resolve(s: &Scenario, res_dir: &Path) -> Resolved {
    let name = s.name.as_deref().unwrap();
    if name.contains('<') {
        return Resolved::Skip("outline-param");
    }
    let base = s.base.as_deref().unwrap_or("");
    let base = base.strip_prefix('/').unwrap_or(base);
    let full = res_dir.join(base).join(name);

    if name.ends_with(".opf") {
        if full.is_file() {
            return Resolved::Path {
                path: wrap::wrap_opf_file(&full, name),
                is_temp: true,
                single_doc_wrap: true,
            };
        }
        return Resolved::Skip("opf-only (missing file)");
    }
    // Case-insensitive on purpose (not just ".epub" exactly): a real
    // fixture uses a mixed-case ".ePub" extension specifically to test
    // PKG-016, and the file must be handed to our binary with its
    // original name/case intact (not renamed via a temp wrap) for that
    // check to see it at all.
    if name.to_ascii_lowercase().ends_with(".epub") {
        if full.is_file() {
            return Resolved::Path {
                path: full,
                is_temp: false,
                single_doc_wrap: false,
            };
        }
        return Resolved::Skip("missing-file");
    }
    if full.is_dir() {
        return Resolved::Path {
            path: wrap::zip_dir(&full),
            is_temp: true,
            single_doc_wrap: false,
        };
    }
    let full_epub = PathBuf::from(format!("{}.epub", full.display()));
    if full_epub.is_file() {
        return Resolved::Path {
            path: full_epub,
            is_temp: false,
            single_doc_wrap: false,
        };
    }
    if full.is_file()
        && (name.ends_with(".xhtml") || name.ends_with(".html") || name.ends_with(".htm"))
    {
        let version = if s.file.to_string_lossy().contains("/epub2/") {
            "2.0"
        } else {
            "3.0"
        };
        if s.as_nav {
            return Resolved::Path {
                path: wrap::wrap_nav_doc(&full, name, version),
                is_temp: true,
                single_doc_wrap: true,
            };
        }
        return Resolved::Path {
            path: wrap::wrap_single_doc(&full, name, version, s.edupub_profile, s.idx_profile),
            is_temp: true,
            single_doc_wrap: true,
        };
    }
    if full.is_file() && name.ends_with(".smil") {
        return Resolved::Path {
            path: wrap::wrap_smil_file(&full, name),
            is_temp: true,
            single_doc_wrap: true,
        };
    }
    if full.is_file() && name.ends_with(".svg") {
        return Resolved::Path {
            path: wrap::wrap_svg_file(&full, name),
            is_temp: true,
            single_doc_wrap: true,
        };
    }
    Resolved::Skip("missing-file")
}

/// Runs epubveri directly, in-process (no subprocess/CLI round-trip),
/// against the given `.epub` file. Returns (all reported ids, error-only
/// ids, rc) mirroring the CLI's own exit-code semantics
/// (`Report::is_valid()`, errors-only).
///
/// Uses `validate_path_with_profile` (not `validate_bytes_with_profile`
/// on manually-read bytes) - PKG-016 (the file's own `.epub` extension
/// case) is a filesystem-level check that only exists on the `_path`
/// entry point, matching what the real CLI (and Python's own
/// subprocess-based harness) actually exercises.
fn run(path: &Path, cli_profile: Option<&str>) -> (Vec<String>, Vec<String>, i32) {
    let report =
        epubveri::validate_path_with_profile(path, cli_profile).expect("read epub for validation");
    let mut ids = Vec::with_capacity(report.messages.len());
    let mut error_ids = Vec::new();
    for m in &report.messages {
        ids.push(m.id.to_string());
        if m.severity == epubveri::report::Severity::Error {
            error_ids.push(m.id.to_string());
        }
    }
    let rc = if report.is_valid() { 0 } else { 1 };
    (ids, error_ids, rc)
}

fn family(id: &str) -> &str {
    id.split('-').next().unwrap_or(id)
}

/// Formats a string list the way Python's `repr(list)` does (single
/// quotes), for cosmetic diff-parity with the original script's output.
fn py_list(items: &[String]) -> String {
    let quoted: Vec<String> = items.iter().map(|s| format!("'{s}'")).collect();
    format!("[{}]", quoted.join(", "))
}

// Dedicated epubcheck IDs our validator emits (reconciled 2026-06-27). RSC-005 is
// deliberately EXCLUDED here: it is epubcheck's RelaxNG/Schematron catch-all
// (~116 corpus cases), so counting it would swamp this precision metric. We DO
// emit RSC-005 for our structural conditions, so those wins still show up in the
// overall exact-ID recall - just not in this "within target" number.
const TARGET_IDS: &[&str] = &[
    "PKG-004", "PKG-006", "PKG-007", "RSC-001", "RSC-002", "RSC-003", "OPF-001", "OPF-002",
    "OPF-030", "OPF-033", "OPF-034", "OPF-043", "OPF-049", "OPF-050",
];

// Harness-artifact message IDs excluded from single-doc-wrap scoring -
// see each wrap function's own doc comment for why each one is a
// wrapping limitation, not a real epubveri defect, in that context.
const SINGLE_DOC_WRAP_EXCLUDED: &[&str] = &[
    "RSC-001", "RSC-007", "RSC-011", "RSC-008", "OPF-014", "RSC-012", "OPF-078",
];

fn run_report(scenarios: &[Scenario], res_dir: &Path) {
    use std::collections::BTreeMap;

    // BTreeMap, not HashMap: deterministic iteration order across runs
    // (Rust's HashMap uses a randomized per-process seed, which made
    // tied family/skip counts print in a different order every run -
    // purely cosmetic, but worth being reproducible).
    let mut skipped: BTreeMap<&'static str, u32> = BTreeMap::new();
    let (mut n_clean, mut n_clean_pass, mut n_clean_fp) = (0u32, 0u32, 0u32);
    let (mut n_err, mut n_detect, mut n_exact) = (0u32, 0u32, 0u32);
    let (mut n_inscope, mut n_inscope_exact) = (0u32, 0u32);
    let mut exp_family: BTreeMap<String, u32> = BTreeMap::new();
    let mut hit_family: BTreeMap<String, u32> = BTreeMap::new();
    let mut fp_examples: Vec<(String, Vec<String>)> = Vec::new();
    let mut miss_examples: Vec<(String, Vec<String>, Vec<String>)> = Vec::new();
    let mut miss_all: Vec<(String, Vec<String>, Vec<String>)> = Vec::new();

    for s in scenarios {
        let resolved = resolve(s, res_dir);
        let (path, is_temp, single_doc_wrap) = match resolved {
            Resolved::Skip(reason) => {
                *skipped.entry(reason).or_insert(0) += 1;
                continue;
            }
            Resolved::Path {
                path,
                is_temp,
                single_doc_wrap,
            } => (path, is_temp, single_doc_wrap),
        };
        let (ids, error_ids, mut rc) = run(&path, s.cli_profile.as_deref());
        if is_temp {
            let _ = std::fs::remove_file(&path);
        }

        let mut reported: BTreeSet<String> = ids.iter().cloned().collect();
        if single_doc_wrap {
            // epubcheck's single-document check mode never resolves
            // cross-file references (there's no "book" to check them
            // against); our synthetic wrap only has the target's own
            // directory siblings, so these IDs are wrapping-harness
            // artifacts here, not real epubveri defects - see each wrap
            // function's own doc comment for the specific reasoning per
            // ID. Drop them from scoring for these scenarios only.
            for id in SINGLE_DOC_WRAP_EXCLUDED {
                reported.remove(*id);
            }
            let error_set: BTreeSet<&str> = error_ids.iter().map(String::as_str).collect();
            rc = if reported.iter().any(|r| error_set.contains(r.as_str())) {
                1
            } else {
                0
            };
        }

        // A scenario can expect only a *warning* (no "errs"), e.g.
        // MED-016 or CSS-003/019 - these were previously falling through
        // to the "should stay clean" bucket below (since that branch
        // only checked errs), which silently mis-scored them as false
        // positives the moment the corresponding check started actually
        // firing. Score errs+warns together here; only genuinely
        // expectation-free scenarios fall to the clean bucket. `rc` (the
        // CLI's exit code) is error-only by design (`Report::is_valid`),
        // so the "detection recall" sub-metric still only means
        // something for scenarios that expect an actual error - exact-ID
        // recall (the more important number) counts warning-only ones
        // correctly either way.
        let expected: BTreeSet<&String> = s.errs.iter().chain(s.warns.iter()).collect();
        if !expected.is_empty() {
            n_err += 1;
            for e in &expected {
                *exp_family.entry(family(e).to_string()).or_insert(0) += 1;
            }
            if !s.errs.is_empty() && rc == 1 {
                n_detect += 1;
            }
            let hit: Vec<&String> = expected
                .iter()
                .filter(|e| reported.contains(e.as_str()))
                .copied()
                .collect();
            if !hit.is_empty() {
                n_exact += 1;
                for e in &hit {
                    *hit_family.entry(family(e).to_string()).or_insert(0) += 1;
                }
            } else {
                // A genuine exact-ID miss: none of this should-error
                // scenario's expected IDs were reported. Capture every one
                // (not just the TARGET-id subset below) for investigation.
                let mut exp_sorted: Vec<String> =
                    expected.iter().map(|s| s.to_string()).collect();
                exp_sorted.sort();
                let got = if ids.is_empty() {
                    vec!["(none)".to_string()]
                } else {
                    ids.clone()
                };
                miss_all.push((s.name.clone().unwrap_or_default(), exp_sorted, got));
            }
            if expected.iter().any(|e| TARGET_IDS.contains(&e.as_str())) {
                n_inscope += 1;
                if !hit.is_empty() {
                    n_inscope_exact += 1;
                } else if miss_examples.len() < 12 {
                    let mut exp_sorted: Vec<String> =
                        expected.iter().map(|s| s.to_string()).collect();
                    exp_sorted.sort();
                    let got = if ids.is_empty() {
                        vec!["(none)".to_string()]
                    } else {
                        ids.clone()
                    };
                    miss_examples.push((s.name.clone().unwrap_or_default(), exp_sorted, got));
                }
            }
        } else if s.clean {
            n_clean += 1;
            if rc == 0 {
                n_clean_pass += 1;
            } else {
                n_clean_fp += 1;
                if fp_examples.len() < 12 {
                    fp_examples.push((s.name.clone().unwrap_or_default(), ids.clone()));
                }
            }
        }
    }

    fn pct(a: u32, b: u32) -> String {
        if b == 0 {
            "n/a".to_string()
        } else {
            format!("{:.1}%", 100.0 * a as f64 / b as f64)
        }
    }

    println!("\n=== epubveri vs epubcheck corpus ===");
    println!("scenarios parsed (with a publication): {}", scenarios.len());
    let mut skipped_sorted: Vec<(&&str, &u32)> = skipped.iter().collect();
    skipped_sorted.sort_by(|a, b| b.1.cmp(a.1));
    let skipped_total: u32 = skipped.values().sum();
    let skipped_str: Vec<String> = skipped_sorted
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    println!("skipped: {skipped_total}  {}", skipped_str.join("  "));

    println!("\n-- should-ERROR cases: {n_err} --");
    println!(
        "  detection recall (flagged any error): {n_detect}/{n_err} = {}",
        pct(n_detect, n_err)
    );
    println!(
        "  exact-ID recall  (same message ID)  : {n_exact}/{n_err} = {}",
        pct(n_exact, n_err)
    );
    println!(
        "  within our TARGET ids ({} ids): {n_inscope_exact}/{n_inscope} = {} exact",
        TARGET_IDS.len(),
        pct(n_inscope_exact, n_inscope)
    );

    println!("\n-- should-be-CLEAN cases: {n_clean} --");
    println!(
        "  passed (we stayed silent): {n_clean_pass}/{n_clean} = {}",
        pct(n_clean_pass, n_clean)
    );
    println!(
        "  FALSE POSITIVES (we errored): {n_clean_fp}/{n_clean} = {}",
        pct(n_clean_fp, n_clean)
    );

    println!("\n-- expected-error families (top) : exact hits / total --");
    let mut fam_sorted: Vec<(&String, &u32)> = exp_family.iter().collect();
    fam_sorted.sort_by(|a, b| b.1.cmp(a.1));
    for (fam, tot) in fam_sorted.into_iter().take(14) {
        let hit = hit_family.get(fam).copied().unwrap_or(0);
        println!("  {fam:<5} {hit:>4} / {tot}");
    }

    if !miss_all.is_empty() {
        println!(
            "\n-- ALL exact-ID MISSES ({} scenarios) --",
            miss_all.len()
        );
        for (name, exp, got) in &miss_all {
            println!(
                "  {name}\n      expected {}  got {}",
                py_list(exp),
                py_list(got)
            );
        }
    }
    if !miss_examples.is_empty() {
        println!("\n-- in-scope MISSES (target id expected, we missed exact) --");
        for (name, exp, got) in &miss_examples {
            println!(
                "  {name}\n      expected {}  got {}",
                py_list(exp),
                py_list(got)
            );
        }
    }
    if !fp_examples.is_empty() {
        println!("\n-- FALSE-POSITIVE examples (clean file, we errored) --");
        for (name, got) in &fp_examples {
            println!("  {name}  ->  {}", py_list(got));
        }
    }
    println!();
}
