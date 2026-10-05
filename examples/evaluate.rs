//! Score cardmend against a synthetic book's truth.json.
//!
//!     cargo run --release --example evaluate -- DIR [--region IN] [--misses 10] [--wrong 10]
//!
//! Pairwise: every pair of entries cardmend puts in one group, against every
//! pair that belongs to the same person. Cluster: a group counts as right
//! only if it is exactly one person's entries, all of them. Traps: pairs of
//! different people from a planted trap (household, family email,
//! switchboard, same name) that ended up in one group.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;

use cardmend::import::Book;
use cardmend::matching::Tier;
use cardmend::{analyze, normalize};
use serde_json::Value;

fn pairs_of(groups: &[Vec<usize>]) -> HashSet<(usize, usize)> {
    let mut out = HashSet::new();
    for g in groups {
        for (i, &a) in g.iter().enumerate() {
            for &b in &g[i + 1..] {
                out.insert((a.min(b), a.max(b)));
            }
        }
    }
    out
}

fn pct(n: usize, d: usize) -> String {
    if d == 0 {
        "n/a".into()
    } else {
        format!("{:.1}%", 100.0 * n as f64 / d as f64)
    }
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = PathBuf::from(
        args.first()
            .ok_or_else(|| anyhow::anyhow!("usage: evaluate DIR [--region IN] [--misses N]"))?,
    );
    let opt = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
    };
    let region = normalize::region(opt("--region").map_or("IN", |s| s.as_str())).unwrap();
    let show_misses: usize = opt("--misses").and_then(|s| s.parse().ok()).unwrap_or(0);
    let show_wrong: usize = opt("--wrong").and_then(|s| s.parse().ok()).unwrap_or(0);

    let truth: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("truth.json"))?)?;
    let files: Vec<String> = truth["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f.as_str().unwrap().to_string())
        .collect();

    let t0 = Instant::now();
    let mut book = Book::default();
    for f in &files {
        book.add(f, &std::fs::read(dir.join(f))?);
    }
    let read = t0.elapsed();
    let t1 = Instant::now();
    let a = analyze(&book.contacts, region, &[]);
    let matched = t1.elapsed();

    // Contact id -> person, through (file, index).
    let mut person_of: HashMap<(String, usize), usize> = HashMap::new();
    let mut damage_of: HashMap<(String, usize), Vec<String>> = HashMap::new();
    for e in truth["entries"].as_array().unwrap() {
        let key = (
            e["file"].as_str().unwrap().to_string(),
            e["index"].as_u64().unwrap() as usize,
        );
        person_of.insert(key.clone(), e["person"].as_u64().unwrap() as usize);
        damage_of.insert(
            key,
            e["damage"]
                .as_array()
                .unwrap()
                .iter()
                .map(|d| d.as_str().unwrap().to_string())
                .collect(),
        );
    }
    let person: Vec<usize> = book
        .contacts
        .iter()
        .map(|c| person_of[&(c.source.file.clone(), c.source.index)])
        .collect();
    assert_eq!(
        book.contacts.len(),
        person_of.len(),
        "every entry read back"
    );

    let mut truth_groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for (id, &p) in person.iter().enumerate() {
        truth_groups.entry(p).or_default().push(id);
    }
    let truth_groups: Vec<Vec<usize>> =
        truth_groups.into_values().filter(|g| g.len() > 1).collect();
    let true_pairs = pairs_of(&truth_groups);
    let truth_sets: HashSet<Vec<usize>> = truth_groups
        .iter()
        .map(|g| {
            let mut g = g.clone();
            g.sort_unstable();
            g
        })
        .collect();

    println!(
        "{} entries for {} people in {} files; {} people have more than one entry ({} true pairs)",
        book.contacts.len(),
        truth["people"],
        files.len(),
        truth_groups.len(),
        true_pairs.len()
    );
    println!("parse issues: {}", book.issues.len());
    for i in book.issues.iter().take(5) {
        println!("  {} line {}: {}", i.file, i.line, i.message);
    }
    println!(
        "time: read {:.3} s, match and group {:.3} s\n",
        read.as_secs_f64(),
        matched.as_secs_f64()
    );

    println!(
        "| merged at | groups | pair precision | pair recall | exact groups | group precision | group recall |"
    );
    println!("|---|---|---|---|---|---|---|");
    for (label, tiers) in [
        ("sure", &[Tier::Sure][..]),
        ("sure + likely", &[Tier::Sure, Tier::Likely]),
        ("all three tiers", &[Tier::Sure, Tier::Likely, Tier::Check]),
    ] {
        let groups: Vec<Vec<usize>> = a
            .groups
            .iter()
            .filter(|g| tiers.contains(&g.tier))
            .map(|g| g.members.clone())
            .collect();
        let pred = pairs_of(&groups);
        let hit = pred.intersection(&true_pairs).count();
        let exact = groups.iter().filter(|g| truth_sets.contains(*g)).count();
        println!(
            "| {label} | {} | {} | {} | {} | {} | {} |",
            groups.len(),
            pct(hit, pred.len()),
            pct(hit, true_pairs.len()),
            exact,
            pct(exact, groups.len()),
            pct(exact, truth_groups.len()),
        );
    }

    // Traps, judged on everything cardmend offered (all tiers).
    let mut group_of = vec![usize::MAX; person.len()];
    for (gi, g) in a.groups.iter().enumerate() {
        for &m in &g.members {
            group_of[m] = gi;
        }
    }
    let ids_of =
        |p: usize| -> Vec<usize> { (0..person.len()).filter(|&i| person[i] == p).collect() };
    println!(
        "\n| trap | pairs of different people | wrongly grouped (any tier) | wrongly grouped as sure |"
    );
    println!("|---|---|---|---|");
    let mut kinds: Vec<&str> = truth["traps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["kind"].as_str().unwrap())
        .collect();
    kinds.sort_unstable();
    kinds.dedup();
    for kind in kinds {
        let (mut total, mut wrong, mut wrong_sure) = (0, 0, 0);
        for t in truth["traps"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|t| t["kind"] == kind)
        {
            let ps: Vec<usize> = t["persons"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| p.as_u64().unwrap() as usize)
                .collect();
            for (i, &p) in ps.iter().enumerate() {
                for &q in &ps[i + 1..] {
                    total += 1;
                    let (ep, eq) = (ids_of(p), ids_of(q));
                    let joined = ep.iter().any(|&x| {
                        eq.iter()
                            .any(|&y| group_of[x] != usize::MAX && group_of[x] == group_of[y])
                    });
                    if joined {
                        wrong += 1;
                        let sure = ep.iter().any(|&x| {
                            eq.iter().any(|&y| {
                                group_of[x] == group_of[y]
                                    && group_of[x] != usize::MAX
                                    && a.groups[group_of[x]].tier == Tier::Sure
                            })
                        });
                        if sure {
                            wrong_sure += 1;
                        }
                    }
                }
            }
        }
        println!("| {kind} | {total} | {wrong} | {wrong_sure} |");
    }

    // Which kinds of damage the missed pairs had, over all tiers.
    let all: Vec<Vec<usize>> = a.groups.iter().map(|g| g.members.clone()).collect();
    let found = pairs_of(&all);
    let mut missed_by: HashMap<String, (usize, usize)> = HashMap::new();
    let key = |id: usize| {
        (
            book.contacts[id].source.file.clone(),
            book.contacts[id].source.index,
        )
    };
    let mut misses = Vec::new();
    for &(x, y) in &true_pairs {
        let mut tags: Vec<String> = damage_of[&key(x)]
            .iter()
            .chain(&damage_of[&key(y)])
            .cloned()
            .collect();
        if tags.is_empty() {
            tags.push("none".into());
        }
        tags.sort();
        tags.dedup();
        let hit = found.contains(&(x, y));
        for t in tags {
            let e = missed_by.entry(t).or_default();
            e.1 += 1;
            if !hit {
                e.0 += 1;
            }
        }
        if !hit {
            misses.push((x, y));
        }
    }
    let mut rows: Vec<_> = missed_by.into_iter().collect();
    rows.sort_by(|a, b| b.1.0.cmp(&a.1.0).then(a.0.cmp(&b.0)));
    println!("\n| damage on either copy | true pairs | missed |");
    println!("|---|---|---|");
    for (tag, (miss, total)) in rows {
        println!("| {tag} | {total} | {miss} ({}) |", pct(miss, total));
    }
    misses.sort_unstable();
    for &(x, y) in misses.iter().take(show_misses) {
        let c = &book.contacts;
        let show = |i: usize| {
            format!(
                "{:?} {:?} {:?}",
                c[i].display_name(),
                c[i].phones.iter().map(|p| &p.value).collect::<Vec<_>>(),
                c[i].emails.iter().map(|p| &p.value).collect::<Vec<_>>()
            )
        };
        println!("missed: {}  |  {}", show(x), show(y));
    }
    // Pairs grouped together that belong to different people, with the
    // tier of their group and the evidence if the pair itself was scored.
    let mut wrong: Vec<(usize, usize)> = found.difference(&true_pairs).copied().collect();
    wrong.sort_unstable();
    for &(x, y) in wrong.iter().take(show_wrong) {
        let c = &book.contacts;
        let tier = a.groups[group_of[x]].tier;
        let why = a
            .pairs
            .iter()
            .find(|p| (p.a.min(p.b), p.a.max(p.b)) == (x, y))
            .map(|p| {
                p.evidence
                    .iter()
                    .map(|e| e.text.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            })
            .unwrap_or_else(|| "joined through another member".into());
        println!(
            "wrong ({tier:?}): {:?} | {:?}: {why}",
            c[x].display_name(),
            c[y].display_name()
        );
    }
    Ok(())
}
