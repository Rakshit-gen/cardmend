use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, bail};
use cardmend::import::Book;
use cardmend::matching::Tier;
use cardmend::merge::Choices;
use cardmend::{analyze, merge, normalize, write};
use clap::{Parser, ValueEnum};
use serde_json::json;

#[derive(Parser)]
#[command(
    version,
    about = "Find duplicate contacts across address book exports and write one clean vCard file",
    after_help = "Without -o nothing is written; you get the report only. Your input files are never changed."
)]
struct Cli {
    /// vCard (.vcf) files and Google or Outlook CSV exports.
    #[arg(required = true)]
    files: Vec<PathBuf>,
    /// Write the merged address book here as vCard 3.0.
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// Country for numbers saved without a country code, like IN or GB.
    /// Defaults to the one in your locale.
    #[arg(long)]
    region: Option<String>,
    /// Merge groups at this tier and above when writing.
    #[arg(long, value_enum, default_value_t = MergeLevel::Sure)]
    merge: MergeLevel,
    /// Print everything found as JSON instead of the report.
    #[arg(long)]
    json: bool,
    /// How many groups to list in the report.
    #[arg(long, default_value_t = 10)]
    top: usize,
}

#[derive(Clone, Copy, ValueEnum)]
enum MergeLevel {
    Sure,
    Likely,
    Check,
}

impl MergeLevel {
    fn includes(self, t: Tier) -> bool {
        match self {
            MergeLevel::Sure => t == Tier::Sure,
            MergeLevel::Likely => t != Tier::Check,
            MergeLevel::Check => true,
        }
    }
}

fn tier_word(t: Tier) -> &'static str {
    match t {
        Tier::Sure => "sure",
        Tier::Likely => "likely",
        Tier::Check => "check",
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let region = match &cli.region {
        Some(r) => normalize::region(r)
            .with_context(|| format!("{r} isn't a two-letter country code like IN, GB or US"))?,
        None => normalize::default_region(),
    };
    let started = Instant::now();
    let mut book = Book::default();
    for path in &cli.files {
        let bytes =
            std::fs::read(path).with_context(|| format!("can't read {}", path.display()))?;
        book.add(&path.display().to_string(), &bytes);
    }
    if book.contacts.is_empty() {
        for i in &book.issues {
            eprintln!("{}: {}", i.file, i.message);
        }
        bail!("no contacts found in the files given");
    }
    let a = analyze(&book.contacts, region, &[]);
    let merges: Vec<(Vec<usize>, Choices)> = a
        .groups
        .iter()
        .filter(|g| cli.merge.includes(g.tier))
        .map(|g| (g.members.clone(), Choices::default()))
        .collect();
    let (clean, summary) = merge::apply(&book.contacts, &merges, region);
    let elapsed = started.elapsed();

    if let Some(out) = &cli.output {
        if cli.files.iter().any(|f| same_file(f, out)) {
            bail!(
                "{} is one of the input files; pick another name so the original stays as it was",
                out.display()
            );
        }
        std::fs::write(out, write::write_all(&clean))
            .with_context(|| format!("can't write {}", out.display()))?;
    }

    let c = &book.contacts;
    if cli.json {
        let groups: Vec<_> = a
            .groups
            .iter()
            .map(|g| {
                json!({
                    "tier": g.tier,
                    "score": (g.score * 100.0).round() / 100.0,
                    "members": g.members.iter().map(|&i| json!({
                        "id": i,
                        "name": c[i].display_name(),
                        "file": c[i].source.file,
                        "line": c[i].source.line,
                    })).collect::<Vec<_>>(),
                    "evidence": g.pairs.iter().map(|&p| json!({
                        "a": a.pairs[p].a,
                        "b": a.pairs[p].b,
                        "why": a.pairs[p].evidence.iter().map(|e| &e.text).collect::<Vec<_>>(),
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        let v = json!({
            "files": book.files,
            "issues": book.issues,
            "region": region.as_ref(),
            "groups": groups,
            "shared": a.shared,
            "problems": a.problems,
            "summary": summary,
        });
        println!("{}", serde_json::to_string_pretty(&v)?);
        return Ok(());
    }

    for i in &book.issues {
        if i.line > 0 {
            eprintln!("{} line {}: {}", i.file, i.line, i.message);
        } else {
            eprintln!("{}: {}", i.file, i.message);
        }
    }
    let files: Vec<String> = book
        .files
        .iter()
        .map(|(f, n)| format!("{f} ({n})"))
        .collect();
    println!(
        "Read {}: {}",
        plural(files.len(), "file", "files"),
        files.join(", ")
    );
    let count = |t| a.groups.iter().filter(|g| g.tier == t).count();
    let took = format!(
        "({:.2} s, numbers read as {})",
        elapsed.as_secs_f64(),
        region.as_ref()
    );
    if a.groups.is_empty() {
        println!("{} contacts and no duplicates found.  {took}", c.len());
    } else {
        println!(
            "{} contacts. {} like duplicates: {} sure, {} likely, {} to check.  {took}",
            c.len(),
            plural(a.groups.len(), "group looks", "groups look"),
            count(Tier::Sure),
            count(Tier::Likely),
            count(Tier::Check),
        );
    }
    for g in a.groups.iter().take(cli.top) {
        println!();
        let base = merge::merge(
            &g.members.iter().map(|&i| &c[i]).collect::<Vec<_>>(),
            region,
            &Choices::default(),
        );
        println!("{:<7}{}", tier_word(g.tier), base.contact.display_name());
        for &m in &g.members {
            println!(
                "       {:<32} {}:{}",
                c[m].display_name(),
                c[m].source.file,
                c[m].source.line
            );
        }
        for &p in &g.pairs {
            let why: Vec<&str> = a.pairs[p]
                .evidence
                .iter()
                .map(|e| e.text.as_str())
                .collect();
            println!("       why: {}", why.join("; "));
        }
    }
    if a.groups.len() > cli.top {
        println!(
            "\n... and {} more. Raise --top to see them.",
            a.groups.len() - cli.top
        );
    }
    let p = &a.problems;
    let mut also = Vec::new();
    if !p.no_name.is_empty() {
        also.push(plural(
            p.no_name.len(),
            "contact with no name",
            "contacts with no name",
        ));
    }
    if !p.no_country.is_empty() {
        also.push(plural(
            p.no_country.len(),
            "number without a country code",
            "numbers without a country code",
        ));
    }
    if !p.empty.is_empty() {
        also.push(plural(p.empty.len(), "empty entry", "empty entries"));
    }
    if !also.is_empty() {
        println!("\nAlso found: {}.", also.join(", "));
    }
    for s in a.shared.iter().take(5) {
        let names: Vec<String> = s
            .contacts
            .iter()
            .take(4)
            .map(|&i| c[i].display_name())
            .collect();
        println!(
            "Shared by different people, so not used to merge: {} ({})",
            s.display,
            names.join(", ")
        );
    }
    println!();
    match &cli.output {
        Some(out) => println!(
            "Wrote {}: {} contacts in, {} out, {} merged{}.",
            out.display(),
            summary.contacts_in,
            summary.contacts_out,
            plural(summary.groups_merged, "group", "groups"),
            if summary.empty_dropped > 0 {
                format!(
                    ", {} left out",
                    plural(summary.empty_dropped, "empty entry", "empty entries")
                )
            } else {
                String::new()
            }
        ),
        None => println!(
            "Nothing written. Add -o clean.vcf to merge the {} groups and write the result.",
            match cli.merge {
                MergeLevel::Sure => "sure",
                MergeLevel::Likely => "sure and likely",
                MergeLevel::Check => "listed",
            }
        ),
    }
    Ok(())
}

fn same_file(a: &std::path::Path, b: &std::path::Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}
