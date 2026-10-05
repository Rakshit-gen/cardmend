//! Finding pairs of contacts that may be the same person, and saying why.

use std::collections::{HashMap, HashSet};

use phonenumber::country::Id;
use serde::Serialize;

use crate::contact::Contact;
use crate::normalize::{self, NameKey, NameKind, NameMatch};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Sure,
    Likely,
    Check,
}

impl Tier {
    pub fn of(score: f32) -> Option<Tier> {
        if score >= 1.0 {
            Some(Tier::Sure)
        } else if score >= 0.7 {
            Some(Tier::Likely)
        } else if score >= 0.4 {
            Some(Tier::Check)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Evidence {
    pub text: String,
    pub weight: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Pair {
    pub a: usize,
    pub b: usize,
    pub score: f32,
    pub names: NameMatch,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone)]
pub struct PhoneKey {
    pub key: String,
    pub display: String,
    pub has_country: bool,
    /// Last nine digits as saved. A foreign number saved without its
    /// country code gets the wrong E.164 key, but its tail still matches.
    pub tail: String,
    /// "mobile", "home number" and so on, for explanations.
    pub kind: &'static str,
}

/// What matching needs from a contact, computed once.
#[derive(Debug, Clone)]
pub struct Prepared {
    pub name: NameKey,
    pub display: String,
    pub phones: Vec<PhoneKey>,
    pub emails: Vec<String>,
    /// (year, month, day); year 0 when unknown.
    pub birthday: Option<(u16, u8, u8)>,
}

fn phone_kind(types: &[String], label: Option<&str>) -> &'static str {
    let has = |t: &str| types.iter().any(|x| x == t);
    let label = label.unwrap_or("").to_ascii_lowercase();
    if has("cell") || label.contains("mobile") || label == "iphone" {
        "mobile"
    } else if has("fax") {
        "fax"
    } else if has("home") || label.contains("home") {
        "home number"
    } else if has("work") || label.contains("work") || label.contains("office") || label == "main" {
        "work number"
    } else {
        "number"
    }
}

fn birthday(s: &str) -> Option<(u16, u8, u8)> {
    if let Some(md) = s.strip_prefix("--") {
        let (m, d) = md.split_once('-')?;
        return Some((0, m.parse().ok()?, d.parse().ok()?));
    }
    let mut p = s.split('-');
    let (y, m, d) = (p.next()?, p.next()?, p.next()?);
    Some((y.parse().ok()?, m.parse().ok()?, d.parse().ok()?))
}

pub fn prepare(c: &Contact, region: Id) -> Prepared {
    let mut phones: Vec<PhoneKey> = Vec::new();
    for p in &c.phones {
        if let Some(n) = normalize::phone(&p.value, region)
            && !phones.iter().any(|x| x.key == n.key)
        {
            let digits: String = p.value.chars().filter(|c| c.is_ascii_digit()).collect();
            phones.push(PhoneKey {
                tail: digits[digits.len().saturating_sub(9)..].to_string(),
                key: n.key,
                display: n.display,
                has_country: n.has_country,
                kind: phone_kind(&p.types, p.label.as_deref()),
            });
        }
    }
    let mut emails: Vec<String> = c
        .emails
        .iter()
        .filter_map(|e| normalize::email(&e.value))
        .collect();
    emails.dedup();
    Prepared {
        name: normalize::name_key(c),
        display: c.display_name(),
        phones,
        emails,
        birthday: c.birthday.as_deref().and_then(birthday),
    }
}

/// Identifiers (phone keys and emails) held by contacts whose names are
/// clearly different people: a household landline, an office switchboard,
/// a family email. They still count, but for little.
#[derive(Debug, Clone, Serialize)]
pub struct Shared {
    pub key: String,
    pub display: String,
    pub contacts: Vec<usize>,
}

/// Blocks bigger than this are skipped for name keys: a key that common
/// ("smith|j" in a huge book) is better caught by a shared number.
/// ponytail: a fixed cap; split big blocks by a second key if recall on
/// very common names ever matters.
const MAX_BLOCK: usize = 300;

/// Contact pairs worth scoring: those sharing a number, an email, or a
/// name key. Name keys are family word + given initial and the reverse, for
/// every reading of the given name, so nicknames, initials, swapped order
/// and a typo in one of the two parts still land in the same block.
pub fn candidates(prep: &[Prepared]) -> Vec<(usize, usize)> {
    let mut blocks: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, p) in prep.iter().enumerate() {
        let mut keys: HashSet<String> = HashSet::new();
        for ph in &p.phones {
            keys.insert(format!("p:{}", ph.key));
            keys.insert(format!("t:{}", ph.tail));
        }
        for e in &p.emails {
            keys.insert(format!("e:{e}"));
        }
        match p.name.kind {
            NameKind::Person => {
                let givens = normalize::canonical(&p.name.given);
                for fam in p.name.family.split(' ').filter(|w| w.len() > 1) {
                    for g in &givens {
                        if let (Some(gi), Some(fi)) = (g.chars().next(), fam.chars().next()) {
                            keys.insert(format!("n:{fam}|{gi}"));
                            keys.insert(format!("n:{g}|{fi}"));
                        }
                    }
                }
            }
            NameKind::Company => {
                keys.insert(format!("c:{}", p.name.org));
            }
            _ => {}
        }
        for k in keys {
            blocks.entry(k).or_default().push(i);
        }
    }
    let mut pairs = HashSet::new();
    for (k, members) in &blocks {
        if members.len() > MAX_BLOCK && k.starts_with(['n', 'c']) {
            continue;
        }
        for (x, &a) in members.iter().enumerate() {
            for &b in &members[x + 1..] {
                pairs.insert((a.min(b), a.max(b)));
            }
        }
    }
    let mut v: Vec<_> = pairs.into_iter().collect();
    v.sort_unstable();
    v
}

pub fn find_shared(prep: &[Prepared]) -> Vec<Shared> {
    let mut holders: HashMap<&str, (String, Vec<usize>)> = HashMap::new();
    for (i, p) in prep.iter().enumerate() {
        for ph in &p.phones {
            holders
                .entry(&ph.key)
                .or_insert_with(|| (ph.display.clone(), Vec::new()))
                .1
                .push(i);
        }
        for e in &p.emails {
            holders
                .entry(e)
                .or_insert_with(|| (e.clone(), Vec::new()))
                .1
                .push(i);
        }
    }
    let mut out: Vec<Shared> = holders
        .into_iter()
        .filter(|(_, (_, who))| who.len() > 1)
        .filter(|(_, (_, who))| {
            who.iter().enumerate().any(|(x, &a)| {
                who[x + 1..].iter().any(|&b| {
                    normalize::compare(&prep[a].name, &prep[b].name) == NameMatch::Different
                })
            })
        })
        .map(|(k, (display, contacts))| Shared {
            key: k.to_string(),
            display,
            contacts,
        })
        .collect();
    out.sort_by(|a, b| a.key.cmp(&b.key));
    out
}

/// True when two contacts can't be the same person whatever else they
/// share: clearly different names, or different birthdays.
pub fn conflict(a: &Prepared, b: &Prepared) -> bool {
    if normalize::compare(&a.name, &b.name) == NameMatch::Different {
        return true;
    }
    matches!((a.birthday, b.birthday), (Some(x), Some(y)) if (x.1, x.2) != (y.1, y.2))
}

fn name_evidence(m: NameMatch, a: &str, b: &str) -> Option<(String, f32)> {
    let both = if a == b {
        a.to_string()
    } else {
        format!("{a} / {b}")
    };
    Some(match m {
        NameMatch::Exact if a == b => (format!("same name, {a}"), 0.5),
        NameMatch::Exact => (format!("same name written differently: {both}"), 0.5),
        NameMatch::Nickname => (format!("names {both} via nickname"), 0.4),
        NameMatch::Swapped => (format!("names {both} with first and last swapped"), 0.4),
        NameMatch::Typo => (format!("names {both} differ by a letter or two"), 0.35),
        NameMatch::Initial => (format!("names {both}: an initial that fits"), 0.3),
        NameMatch::Similar => (
            format!("names {both}: same first name, close last names"),
            0.2,
        ),
        NameMatch::Partial => (format!("names {both}: only one name to compare"), 0.15),
        NameMatch::FamilyDiffers => (
            format!("same first name but different last names: {both}"),
            -0.3,
        ),
        NameMatch::Different => (format!("different names: {both}"), -1.0),
        NameMatch::Unknown => return None,
    })
}

pub fn score(i: usize, j: usize, a: &Prepared, b: &Prepared, shared: &HashSet<&str>) -> Pair {
    let mut ev: Vec<Evidence> = Vec::new();
    let mut add = |text: String, weight: f32| ev.push(Evidence { text, weight });

    let names = normalize::compare(&a.name, &b.name);
    if let Some((t, w)) = name_evidence(names, &a.display, &b.display) {
        add(t, w);
    }

    let common_phones: Vec<&PhoneKey> = a
        .phones
        .iter()
        .filter(|p| b.phones.iter().any(|q| q.key == p.key))
        .collect();
    let mut strong = 0;
    for p in &common_phones {
        if shared.contains(p.key.as_str()) {
            add(
                format!(
                    "both have {}, but so do contacts with other names, so it counts for little",
                    p.display
                ),
                0.1,
            );
        } else {
            strong += 1;
            add(
                format!("same {} {}", p.kind, p.display),
                if strong == 1 { 0.6 } else { 0.1 },
            );
        }
    }
    // A number saved without a country code read with the wrong country.
    let tail_match = a.phones.iter().find_map(|p| {
        b.phones
            .iter()
            .find(|q| {
                p.key != q.key
                    && p.tail == q.tail
                    && p.tail.len() == 9
                    && (!p.has_country || !q.has_country)
            })
            .map(|q| if p.has_country { (q, p) } else { (p, q) })
    });
    if common_phones.is_empty()
        && let Some((local, intl)) = tail_match
    {
        add(
            format!(
                "same number if {} is {} (one copy has no country code)",
                local.display, intl.display
            ),
            0.45,
        );
    }
    if common_phones.is_empty()
        && tail_match.is_none()
        && !a.phones.is_empty()
        && !b.phones.is_empty()
    {
        add("no number in common".into(), -0.15);
    }

    let common_emails: Vec<&String> = a.emails.iter().filter(|e| b.emails.contains(e)).collect();
    let mut strong_e = 0;
    for e in &common_emails {
        if shared.contains(e.as_str()) {
            add(
                format!(
                    "both have {e}, but so do contacts with other names, so it counts for little"
                ),
                0.1,
            );
        } else {
            strong_e += 1;
            add(
                format!("same email {e}"),
                if strong_e == 1 { 0.5 } else { 0.1 },
            );
        }
    }
    if common_emails.is_empty() && !a.emails.is_empty() && !b.emails.is_empty() {
        add("no email in common".into(), -0.15);
    }

    match (a.birthday, b.birthday) {
        (Some(x), Some(y)) if (x.1, x.2) != (y.1, y.2) => add("different birthdays".into(), -1.0),
        (Some(x), Some(y)) if x.0 != 0 && x.0 == y.0 => add("same birthday".into(), 0.3),
        (Some(_), Some(_)) => add("same birthday (day and month)".into(), 0.15),
        _ => {}
    }

    if !a.name.org.is_empty()
        && a.name.org == b.name.org
        && a.name.kind == NameKind::Person
        && b.name.kind == NameKind::Person
    {
        add("same company".into(), 0.1);
    }

    let score = ev.iter().map(|e| e.weight).sum();
    Pair {
        a: i,
        b: j,
        score,
        names,
        evidence: ev,
    }
}

/// Scored pairs that reach at least the "check" tier.
pub fn pairs(prep: &[Prepared], shared: &[Shared]) -> Vec<Pair> {
    let shared: HashSet<&str> = shared.iter().map(|s| s.key.as_str()).collect();
    candidates(prep)
        .into_iter()
        .map(|(i, j)| score(i, j, &prep[i], &prep[j], &shared))
        .filter(|p| Tier::of(p.score).is_some())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contact::Field;

    pub fn person(name: &str, phones: &[&str], emails: &[&str]) -> Contact {
        Contact {
            formatted_name: name.into(),
            phones: phones
                .iter()
                .map(|p| Field::typed(p.to_string(), &["cell"]))
                .collect(),
            emails: emails.iter().map(|e| Field::new(e.to_string())).collect(),
            ..Contact::default()
        }
    }

    fn prep(cs: &[Contact]) -> Vec<Prepared> {
        cs.iter().map(|c| prepare(c, Id::IN)).collect()
    }

    #[test]
    fn explains_a_nickname_match_on_a_mobile() {
        let p = prep(&[
            person("Bob Smith (work)", &["098765 43210"], &[]),
            person("Robert Smith", &["+91 98765 43210"], &[]),
        ]);
        let pair = score(0, 1, &p[0], &p[1], &HashSet::new());
        let texts: Vec<_> = pair.evidence.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(
            texts,
            [
                "names Bob Smith (work) / Robert Smith via nickname",
                "same mobile +91 98765 43210"
            ]
        );
        assert_eq!(Tier::of(pair.score), Some(Tier::Sure));
    }

    #[test]
    fn blocks_find_swapped_and_typo_names_without_shared_numbers() {
        let p = prep(&[
            person("Robert Smith", &[], &[]),
            person("Smith Robert", &[], &[]),
            person("Robert Smtih", &[], &[]),
            person("Alice Jones", &[], &[]),
        ]);
        let c = candidates(&p);
        assert!(c.contains(&(0, 1)));
        assert!(c.contains(&(0, 2)));
        assert!(!c.iter().any(|&(a, b)| a == 3 || b == 3));
    }

    #[test]
    fn household_landline_is_shared_and_counts_for_little() {
        let cs = [
            person("Priya Shah", &["+91 98765 43210", "022 2345 6789"], &[]),
            person("Rahul Shah", &["+91 98111 22334", "022 2345 6789"], &[]),
            person("Priya Shah", &["022 2345 6789"], &[]),
        ];
        let p = prep(&cs);
        let shared = find_shared(&p);
        assert_eq!(shared.len(), 1);
        assert_eq!(shared[0].display, "+91 22 2345 6789");
        let found = pairs(&p, &shared);
        // Priya and Rahul aren't paired; Priya's two copies still are.
        assert!(!found.iter().any(|x| (x.a, x.b) == (0, 1)));
        let pr = found.iter().find(|x| (x.a, x.b) == (0, 2)).unwrap();
        assert!(pr.evidence[1].text.contains("counts for little"));
    }

    #[test]
    fn same_common_name_with_different_details_is_not_a_pair() {
        let p = prep(&[
            person("John Smith", &["+1 415 555 2671"], &["john@a.example"]),
            person("John Smith", &["+1 212 555 0199"], &["jsmith@b.example"]),
        ]);
        assert!(pairs(&p, &[]).is_empty());
    }

    #[test]
    fn foreign_number_saved_without_country_code() {
        let p = prep(&[
            person("Abby Green", &["(617) 561-7230"], &[]),
            person("Abigail Green", &["+1 617-561-7230"], &[]),
            person("Unrelated", &["+1 617-561-7230"], &[]),
        ]);
        assert!(
            candidates(&p).contains(&(0, 2)),
            "tail block, not just the name block"
        );
        assert!(candidates(&p).contains(&(0, 1)));
        let pair = score(0, 1, &p[0], &p[1], &HashSet::new());
        assert!(
            pair.evidence[1]
                .text
                .contains("one copy has no country code"),
            "{:?}",
            pair.evidence
        );
        assert_eq!(Tier::of(pair.score), Some(Tier::Likely));
    }

    #[test]
    fn conflicts() {
        let mut a = person("Priya Shah", &[], &[]);
        let mut b = person("P. Shah", &[], &[]);
        let p = prep(&[a.clone(), b.clone()]);
        assert!(!conflict(&p[0], &p[1]));
        a.birthday = Some("1990-01-02".into());
        b.birthday = Some("--03-04".into());
        let p = prep(&[a, b]);
        assert!(conflict(&p[0], &p[1]));
    }
}
