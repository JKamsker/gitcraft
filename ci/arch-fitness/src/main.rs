//! `gitcraft-arch-fitness check|report|ratchet|adopt`: see docs/architecture-boundaries.md.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use gitcraft_arch_fitness::{
    Analysis, Baseline, Outcome, Policy, Report, markdown, policy, ratchet, source,
};

const USAGE: &str =
    "usage: gitcraft-arch-fitness <check|report|ratchet|adopt> [--root DIR] [--revision REV]
  [--policy FILE] [--baseline FILE] [--json FILE] [--markdown FILE] [--dot FILE]
  adopt also needs --reason TEXT --owner TEXT --remediation TEXT";

#[derive(Default)]
struct Args {
    command: String,
    root: Option<PathBuf>,
    revision: Option<String>,
    policy: Option<PathBuf>,
    baseline: Option<PathBuf>,
    json: Option<PathBuf>,
    markdown: Option<PathBuf>,
    dot: Option<PathBuf>,
    reason: Option<String>,
    owner: Option<String>,
    remediation: Option<String>,
}

fn parse(mut argv: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut args = Args {
        command: argv.next().ok_or(USAGE)?,
        ..Args::default()
    };
    while let Some(flag) = argv.next() {
        let mut value = || argv.next().ok_or(format!("{flag} needs a value"));
        match flag.as_str() {
            "--root" => args.root = Some(value()?.into()),
            "--revision" => args.revision = Some(value()?),
            "--policy" => args.policy = Some(value()?.into()),
            "--baseline" => args.baseline = Some(value()?.into()),
            "--json" => args.json = Some(value()?.into()),
            "--markdown" => args.markdown = Some(value()?.into()),
            "--dot" => args.dot = Some(value()?.into()),
            "--reason" => args.reason = Some(value()?),
            "--owner" => args.owner = Some(value()?),
            "--remediation" => args.remediation = Some(value()?),
            other => return Err(format!("unknown argument {other}\n{USAGE}")),
        }
    }
    Ok(args)
}

fn toplevel() -> Result<PathBuf, String> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err("not inside a Git repository; pass --root".into());
    }
    Ok(PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()))
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

fn report(args: &Args, a: &Analysis, outcome: &Outcome) -> Result<(), String> {
    let json = policy::to_json(&Report {
        analysis: a,
        check: Some(outcome),
    })?;
    match &args.json {
        Some(path) => write(path, &json)?,
        None if args.command == "report" && args.markdown.is_none() => print!("{json}"),
        None => {}
    }
    if let Some(path) = &args.markdown {
        write(path, &markdown::render(a, Some(outcome)))?;
    }
    if let Some(path) = &args.dot {
        write(path, &markdown::dot(a))?;
    }
    Ok(())
}

fn print_violations(outcome: &Outcome) {
    let github = std::env::var_os("GITHUB_ACTIONS").is_some();
    for v in &outcome.violations {
        let at = v.location.as_deref().unwrap_or("-");
        let symbol = v
            .symbol
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(|s| format!("\n  symbol: {s}"))
            .unwrap_or_default();
        eprintln!(
            "[{}] {}\n  at: {at}{symbol}\n  fix: {}",
            v.rule, v.message, v.fix
        );
        if let (true, Some((file, line))) = (
            github,
            v.location.as_deref().and_then(|l| l.split_once(':')),
        ) {
            let line: String = line.chars().take_while(char::is_ascii_digit).collect();
            eprintln!(
                "::error file={file},line={line},title=architecture {}::{}",
                v.rule, v.message
            );
        }
    }
}

fn run(args: &Args) -> Result<bool, String> {
    let root = match &args.root {
        Some(root) => root.clone(),
        None => toplevel()?,
    };
    let policy_path = args
        .policy
        .clone()
        .unwrap_or_else(|| root.join("ci/arch-fitness/policy.json"));
    let baseline_path = args
        .baseline
        .clone()
        .unwrap_or_else(|| root.join("ci/arch-fitness/baseline.json"));
    let policy: Policy = policy::read_json(&policy_path)?;
    let baseline: Baseline = policy::read_json(&baseline_path)?;
    let (sources, info) = match &args.revision {
        Some(rev) => source::revision(&root, rev)?,
        None => source::working_tree(&root)?,
    };
    let analysis = gitcraft_arch_fitness::analyze(&sources, &policy, info);
    let outcome = gitcraft_arch_fitness::check(&analysis, &policy, &baseline);
    match args.command.as_str() {
        "check" | "report" => {
            report(args, &analysis, &outcome)?;
            print_violations(&outcome);
            for note in &outcome.ratchet {
                eprintln!("ratchet available: {note}");
            }
            let s = &analysis.summary;
            eprintln!(
                "architecture: {} domains, {} edges ({} forbidden, baselined or not), {} cycles, {} cross-feature RepoTab mutations; {} violations",
                s.domains,
                s.domain_edges,
                s.forbidden_edges,
                s.cycles,
                analysis.repo_tab.cross_mutations.len(),
                outcome.violations.len()
            );
            Ok(args.command == "report" || outcome.violations.is_empty())
        }
        "ratchet" | "adopt" => {
            ratchet::refuse_in_ci()?;
            let new = if args.command == "ratchet" {
                ratchet::ratchet(&analysis, &policy.symbol_budget_domains, &baseline)
            } else {
                let why = ratchet::Why {
                    reason: args.reason.clone().ok_or("adopt needs --reason")?,
                    owner: args.owner.clone().ok_or("adopt needs --owner")?,
                    remediation: args
                        .remediation
                        .clone()
                        .ok_or("adopt needs --remediation")?,
                };
                ratchet::adopt(&analysis, &policy.symbol_budget_domains, &baseline, &why)
            };
            write(&baseline_path, &policy::to_json(&new)?)?;
            eprintln!(
                "wrote {}; review the diff before committing",
                baseline_path.display()
            );
            Ok(true)
        }
        other => Err(format!("unknown command {other}\n{USAGE}")),
    }
}

fn main() -> ExitCode {
    let result = parse(std::env::args().skip(1)).and_then(|args| run(&args));
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("architecture: {e}");
            ExitCode::from(2)
        }
    }
}
