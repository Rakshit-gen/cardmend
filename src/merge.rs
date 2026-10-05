//! Merging a group of contacts into one.
//!
//! Lists (numbers, emails, addresses, websites, categories) are combined
//! and de-duplicated on their normalised value, keeping every label. Fields
//! that hold one value (name, birthday, organisation, job title, photo) get
//! a default pick and the other values are offered as alternatives.

use std::collections::HashSet;

use phonenumber::country::Id;
use serde::{Deserialize, Serialize};

use crate::contact::{Contact, Extra, Field};
use crate::normalize::{self, NameKind};

/// Which member (by contact id) to take each single-valued field from.
/// Missing entries use the default pick.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choices {
    #[serde(default)]
    pub name: Option<usize>,
    #[serde(default)]
    pub birthday: Option<usize>,
    #[serde(default)]
    pub org: Option<usize>,
    #[serde(default)]
    pub title: Option<usize>,
    #[serde(default)]
    pub photo: Option<usize>,
}

/// One distinct value for a single-valued field and the members that have it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Alternative {
    pub from: Vec<usize>,
    pub value: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Alternatives {
    pub name: Vec<Alternative>,
    pub birthday: Vec<Alternative>,
    pub org: Vec<Alternative>,
    pub title: Vec<Alternative>,
    pub photo: Vec<Alternative>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Merged {
    pub contact: Contact,
    /// The picks actually used, defaults filled in.
    pub choices: Choices,
    pub alternatives: Alternatives,
}

/// Distinct values of a field, in member order, with who has each.
fn distinct(
    members: &[&Contact],
    get: impl Fn(&Contact) -> String,
    key: impl Fn(&str) -> String,
) -> Vec<Alternative> {
    let mut out: Vec<(String, Alternative)> = Vec::new();
    for c in members {
        let v = get(c);
        if v.trim().is_empty() {
            continue;
        }
        let k = key(&v);
        match out.iter_mut().find(|(x, _)| *x == k) {
            Some((_, alt)) => alt.from.push(c.id),
            None => out.push((
                k,
                Alternative {
                    from: vec![c.id],
                    value: v,
                },
            )),
        }
    }
    out.into_iter().map(|(_, a)| a).collect()
}

/// How good a name is to keep: a real person's full name, written without
/// notes like "(work)", beats a nickname, a relation word or a lone initial.
fn name_quality(c: &Contact) -> i32 {
    let key = normalize::name_key(c);
    let shown = c.display_name();
    let mut q = match key.kind {
        NameKind::Person => 10,
        NameKind::Company => 6,
        NameKind::Role => 3,
        NameKind::None => 0,
    };
    if !key.family.is_empty() {
        q += 4;
    }
    // "C. Wood" or "Wright M.": an initial is a worse name to keep than the
    // spelled-out one, wherever it sits.
    if key.given.chars().count() == 1 || key.family.chars().count() == 1 {
        q -= 5;
    }
    // "kavita dutta" or "KAVITA DUTTA": typed in a hurry or by an old
    // phone; the normally capitalised copy reads better.
    let letters: Vec<char> = shown.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.len() > 1
        && (letters.iter().all(|c| c.is_lowercase()) || letters.iter().all(|c| c.is_uppercase()))
    {
        q -= 1;
    }
    // A title is fine as a prefix field but clutters the name people see.
    if normalize::starts_with_title(&shown) {
        q -= 1;
    }
    if shown.contains(['(', '[']) {
        q -= 3;
    }
    if normalize::canonical(&key.given).len() > 1 {
        // A nickname: the full name reads better on a card.
        q -= 2;
    }
    q + key.given.chars().count().min(8) as i32 / 3
}

/// How many of the other copies back this name up: the same surname, the
/// same first name, or a nickname for it. Breaks ties towards the form most
/// copies use, so a swapped "Williams Patricia" or a typo loses to the
/// spelling the rest agree on.
fn agreement(c: &Contact, members: &[&Contact]) -> i32 {
    let k = normalize::name_key(c);
    let mut n = 0;
    for o in members.iter().filter(|o| o.id != c.id) {
        let ok = normalize::name_key(o);
        if !k.family.is_empty() && ok.family == k.family {
            n += 1;
        }
        if !k.given.is_empty() && ok.given == k.given {
            n += 1;
        } else if !k.given.is_empty()
            && normalize::canonical(&ok.given)[1..].contains(&k.given.as_str())
        {
            n += 1;
        }
    }
    n
}

/// Members with the most common value first; ties go to the earlier one.
fn most_common(alts: &[Alternative], prefer: impl Fn(&Alternative) -> i32) -> Option<usize> {
    alts.iter()
        .enumerate()
        .max_by_key(|(i, a)| (prefer(a), a.from.len(), usize::MAX - i))
        .map(|(_, a)| a.from[0])
}

fn pick(chosen: Option<usize>, alts: &[Alternative], default: Option<usize>) -> Option<usize> {
    chosen
        .filter(|id| alts.iter().any(|a| a.from.contains(id)))
        .or(default)
}

pub fn merge(members: &[&Contact], region: Id, choices: &Choices) -> Merged {
    assert!(!members.is_empty(), "merge needs at least one contact");
    let by_id = |id: usize| members.iter().find(|c| c.id == id).copied();

    let alternatives = Alternatives {
        // Best-written copy first, so "kavita dutta" and "Kavita Dutta"
        // show as the latter.
        name: {
            let mut best: Vec<&Contact> = members.to_vec();
            best.sort_by_key(|c| std::cmp::Reverse(name_quality(c)));
            distinct(&best, |c| c.display_name(), normalize::fold)
        },
        birthday: distinct(
            members,
            |c| c.birthday.clone().unwrap_or_default(),
            |s| s.to_string(),
        ),
        org: distinct(
            members,
            |c| match c.department.is_empty() {
                true => c.org.clone(),
                false => format!("{}, {}", c.org, c.department),
            },
            normalize::fold,
        ),
        title: distinct(members, |c| c.title.clone(), normalize::fold),
        photo: {
            let mut v: Vec<Alternative> = members
                .iter()
                .filter_map(|c| {
                    c.photo.as_ref().map(|p| Alternative {
                        from: vec![c.id],
                        value: match p {
                            crate::contact::Photo::Data { bytes, .. } => {
                                format!("{} KB", bytes.len().div_ceil(1024))
                            }
                            crate::contact::Photo::Uri(_) => "link".into(),
                        },
                    })
                })
                .collect();
            // Largest first, so the default is the best photo.
            v.sort_by_key(|a| {
                std::cmp::Reverse(
                    by_id(a.from[0])
                        .and_then(|c| c.photo.as_ref())
                        .map_or(0, |p| p.size()),
                )
            });
            v
        },
    };

    let resolved = Choices {
        name: pick(
            choices.name,
            &alternatives.name,
            members
                .iter()
                .max_by_key(|c| (name_quality(c) + agreement(c, members), usize::MAX - c.id))
                .map(|c| c.id),
        ),
        birthday: pick(
            choices.birthday,
            &alternatives.birthday,
            most_common(&alternatives.birthday, |a| {
                i32::from(!a.value.starts_with("--"))
            }),
        ),
        org: pick(
            choices.org,
            &alternatives.org,
            most_common(&alternatives.org, |_| 0),
        ),
        title: pick(
            choices.title,
            &alternatives.title,
            most_common(&alternatives.title, |_| 0),
        ),
        photo: pick(
            choices.photo,
            &alternatives.photo,
            alternatives.photo.first().map(|a| a.from[0]),
        ),
    };

    let base = resolved.name.and_then(by_id).unwrap_or(members[0]);
    let mut out = Contact {
        id: members[0].id,
        source: members[0].source.clone(),
        formatted_name: base.formatted_name.clone(),
        name: base.name.clone(),
        uid: base
            .uid
            .clone()
            .or_else(|| members.iter().find_map(|c| c.uid.clone())),
        ..Contact::default()
    };
    if let Some(c) = resolved.birthday.and_then(by_id) {
        out.birthday = c.birthday.clone();
    }
    if let Some(c) = resolved.org.and_then(by_id) {
        out.org = c.org.clone();
        out.department = c.department.clone();
    }
    if let Some(c) = resolved.title.and_then(by_id) {
        out.title = c.title.clone();
    }
    if let Some(c) = resolved.photo.and_then(by_id) {
        out.photo = c.photo.clone();
    }

    // Nicknames, plus first names from the copies not chosen ("Bob" when
    // "Robert Smith" is kept), so searching for either still finds them.
    let base_given = normalize::name_key(base).given;
    let mut nick_seen: HashSet<String> = HashSet::new();
    nick_seen.insert(base_given.clone());
    for c in members {
        let mut names = c.nicknames.clone();
        let k = normalize::name_key(c);
        // Only real nicknames: a typo or a surname from a swapped copy
        // would just be noise in the nickname field.
        let nick = |a: &str, b: &str| {
            let (ca, cb) = (normalize::canonical(a), normalize::canonical(b));
            ca.iter().any(|x| cb.contains(x))
        };
        if k.kind == NameKind::Person && k.given != base_given && nick(&k.given, &base_given) {
            let original = c
                .display_name()
                .split_whitespace()
                .find(|w| normalize::fold(w) == k.given)
                .map(String::from);
            names.extend(original);
        }
        for n in names {
            if nick_seen.insert(normalize::fold(&n)) {
                out.nicknames.push(n);
            }
        }
    }

    for c in members {
        for p in &c.phones {
            let key = normalize::phone(&p.value, region)
                .map_or_else(|| p.value.trim().to_string(), |n| n.key);
            let found = out.phones.iter().position(|q| {
                normalize::phone(&q.value, region)
                    .map_or_else(|| q.value.trim().to_string(), |n| n.key)
                    == key
            });
            match found {
                Some(i) => {
                    let q = &mut out.phones[i];
                    // Keep the copy with a country code; it works when travelling.
                    if !q.value.trim_start().starts_with('+')
                        && p.value.trim_start().starts_with('+')
                    {
                        q.value = p.value.clone();
                    }
                    union_labels(q, p);
                }
                None => out.phones.push(p.clone()),
            }
        }
        merge_list(&mut out.emails, &c.emails, |v| {
            normalize::email(v).unwrap_or_else(|| v.trim().to_lowercase())
        });
        merge_list(&mut out.urls, &c.urls, |v| {
            let v = v.trim().to_lowercase();
            let v = v
                .trim_start_matches("https://")
                .trim_start_matches("http://")
                .trim_start_matches("www.");
            v.trim_end_matches('/').to_string()
        });
        for a in &c.addresses {
            let key = normalize::fold(&a.value.display());
            match out
                .addresses
                .iter()
                .position(|x| normalize::fold(&x.value.display()) == key)
            {
                Some(i) => union_labels(&mut out.addresses[i], a),
                None => out.addresses.push(a.clone()),
            }
        }
        for cat in &c.categories {
            if !out.categories.iter().any(|x| x.eq_ignore_ascii_case(cat)) {
                out.categories.push(cat.clone());
            }
        }
    }

    out.note = merge_notes(members.iter().map(|c| c.note.as_str()));
    out.extra = merge_extras(members);
    Merged {
        contact: out,
        choices: resolved,
        alternatives,
    }
}

fn union_labels<T>(into: &mut Field<T>, from: &Field<T>) {
    for t in &from.types {
        if !into.types.contains(t) {
            into.types.push(t.clone());
        }
    }
    if into.label.is_none() {
        into.label = from.label.clone();
    }
}

fn merge_list(into: &mut Vec<Field<String>>, from: &[Field<String>], key: impl Fn(&str) -> String) {
    for f in from {
        let k = key(&f.value);
        match into.iter().position(|x| key(&x.value) == k) {
            Some(i) => union_labels(&mut into[i], f),
            None => into.push(f.clone()),
        }
    }
}

/// Notes joined with a blank line, skipping any note (or paragraph) already
/// contained in what's been kept, ignoring case, accents and spacing.
pub fn merge_notes<'a>(notes: impl Iterator<Item = &'a str>) -> String {
    let mut kept: Vec<&str> = Vec::new();
    for note in notes {
        for para in note.split("\n\n").map(str::trim).filter(|p| !p.is_empty()) {
            let f = normalize::fold(para);
            if kept.iter().any(|k| normalize::fold(k).contains(&f)) {
                continue;
            }
            // A longer version replaces a shorter one it contains.
            kept.retain(|k| !f.contains(&normalize::fold(k)));
            kept.push(para);
        }
    }
    kept.join("\n\n")
}

/// Properties kept as-is, without exact repeats. Grouped ones (an Apple
/// anniversary and its label) are renamed when two members used the same
/// group name for different things.
fn merge_extras(members: &[&Contact]) -> Vec<Extra> {
    let mut out: Vec<Extra> = Vec::new();
    let mut used: HashSet<String> = HashSet::new();
    let mut fresh = 0;
    for c in members {
        let mut groups: Vec<&str> = c.extra.iter().filter_map(|e| e.group.as_deref()).collect();
        groups.dedup();
        for e in c.extra.iter().filter(|e| e.group.is_none()) {
            if !out.contains(e) {
                out.push(e.clone());
            }
        }
        for g in groups {
            let set: Vec<&Extra> = c
                .extra
                .iter()
                .filter(|e| e.group.as_deref() == Some(g))
                .collect();
            let same = |o: &Extra, e: &Extra| {
                o.name == e.name && o.value == e.value && o.params == e.params
            };
            let already = out
                .iter()
                .filter(|o| o.group.is_some())
                .any(|o| set.iter().any(|e| same(o, e)));
            if already {
                continue;
            }
            let name = if used.contains(g) {
                loop {
                    fresh += 1;
                    let n = format!("cm{fresh}");
                    if !used.contains(&n) {
                        break n;
                    }
                }
            } else {
                g.to_string()
            };
            used.insert(name.clone());
            out.extend(set.into_iter().map(|e| Extra {
                group: Some(name.clone()),
                ..e.clone()
            }));
        }
    }
    out
}

/// What the clean file will contain, and how it got there.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Summary {
    pub contacts_in: usize,
    pub contacts_out: usize,
    pub groups_merged: usize,
    pub empty_dropped: usize,
}

/// The clean address book: each merged group becomes one contact at the
/// position of its first member, everything else stays as it was, and
/// entries with nothing in them are dropped.
pub fn apply(
    contacts: &[Contact],
    merges: &[(Vec<usize>, Choices)],
    region: Id,
) -> (Vec<Contact>, Summary) {
    let mut merged_into: Vec<Option<usize>> = vec![None; contacts.len()];
    let mut groups = 0;
    for (gi, (ids, _)) in merges.iter().enumerate() {
        let ids: Vec<usize> = ids
            .iter()
            .copied()
            .filter(|&i| i < contacts.len() && merged_into[i].is_none())
            .collect();
        if ids.len() < 2 {
            continue;
        }
        groups += 1;
        for i in ids {
            merged_into[i] = Some(gi);
        }
    }
    let mut out = Vec::new();
    let mut done = vec![false; merges.len()];
    let mut empty = 0;
    for c in contacts {
        match merged_into[c.id] {
            Some(g) if !done[g] => {
                done[g] = true;
                let members: Vec<&Contact> = contacts
                    .iter()
                    .filter(|x| merged_into[x.id] == Some(g))
                    .collect();
                out.push(merge(&members, region, &merges[g].1).contact);
            }
            Some(_) => {}
            None if c.is_empty() => empty += 1,
            None => out.push(c.clone()),
        }
    }
    let summary = Summary {
        contacts_in: contacts.len(),
        contacts_out: out.len(),
        groups_merged: groups,
        empty_dropped: empty,
    };
    (out, summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contact::{Name, Photo};

    fn c(id: usize, name: &str) -> Contact {
        Contact {
            id,
            formatted_name: name.into(),
            ..Contact::default()
        }
    }

    #[test]
    fn unions_numbers_by_normalised_value_and_keeps_labels() {
        let mut a = c(0, "Bob Smith (work)");
        a.phones
            .push(Field::typed("098765 43210".into(), &["cell"]));
        a.emails
            .push(Field::typed("Rob.Smith@gmail.com".into(), &["home"]));
        let mut b = c(1, "Robert Smith");
        let mut p = Field::typed("+91 98765 43210".to_string(), &["voice"]);
        p.label = Some("Personal".into());
        b.phones.push(p);
        b.phones
            .push(Field::typed("+91 22 2345 6789".into(), &["home"]));
        b.emails
            .push(Field::typed("robsmith@gmail.com".into(), &["work"]));
        let m = merge(&[&a, &b], Id::IN, &Choices::default());
        let out = m.contact;
        assert_eq!(out.formatted_name, "Robert Smith");
        assert_eq!(out.nicknames, ["Bob"]);
        assert_eq!(out.phones.len(), 2);
        assert_eq!(out.phones[0].value, "+91 98765 43210");
        assert_eq!(out.phones[0].types, ["cell", "voice"]);
        assert_eq!(out.phones[0].label.as_deref(), Some("Personal"));
        assert_eq!(out.emails.len(), 1);
        assert_eq!(out.emails[0].value, "Rob.Smith@gmail.com");
        assert_eq!(out.emails[0].types, ["home", "work"]);
        assert_eq!(m.alternatives.name.len(), 2);
        assert_eq!(m.choices.name, Some(1));
    }

    #[test]
    fn user_choice_wins_and_bad_choice_falls_back() {
        let a = c(4, "Bob Smith");
        let b = c(9, "Robert Smith");
        let pick = |name| {
            merge(
                &[&a, &b],
                Id::IN,
                &Choices {
                    name,
                    ..Choices::default()
                },
            )
            .contact
            .formatted_name
        };
        assert_eq!(pick(Some(4)), "Bob Smith");
        assert_eq!(pick(Some(77)), "Robert Smith");
    }

    #[test]
    fn single_valued_defaults() {
        let mut a = c(0, "Ann Lee");
        a.birthday = Some("--03-07".into());
        a.org = "Acme".into();
        a.photo = Some(Photo::Data {
            mime: "image/jpeg".into(),
            bytes: vec![1; 100],
        });
        let mut b = c(1, "Ann Lee");
        b.birthday = Some("1990-03-07".into());
        b.org = "ACME".into();
        b.title = "Engineer".into();
        b.photo = Some(Photo::Data {
            mime: "image/jpeg".into(),
            bytes: vec![2; 5000],
        });
        let mut d = c(2, "Ann Lee");
        d.org = "Globex".into();
        d.photo = Some(Photo::Uri("https://example.com/p.jpg".into()));
        let m = merge(&[&a, &b, &d], Id::IN, &Choices::default());
        assert_eq!(
            m.contact.birthday.as_deref(),
            Some("1990-03-07"),
            "the one with a year"
        );
        assert_eq!(m.contact.org, "Acme", "most common, first spelling");
        assert_eq!(m.alternatives.org.len(), 2);
        assert_eq!(m.contact.title, "Engineer");
        assert_eq!(
            m.contact.photo.as_ref().unwrap().size(),
            5000,
            "largest photo"
        );
        assert_eq!(m.alternatives.photo[0].value, "5 KB");
    }

    #[test]
    fn name_quality_prefers_a_real_full_name() {
        let mom = c(0, "Mom");
        let mut full = c(1, "");
        full.name = Name {
            given: "Sunita".into(),
            family: "Shah".into(),
            ..Name::default()
        };
        let m = merge(&[&mom, &full], Id::IN, &Choices::default());
        assert_eq!(m.contact.name.given, "Sunita");
        assert_eq!(m.alternatives.name.len(), 2);
    }

    #[test]
    fn name_quality_avoids_initials() {
        let mut short = c(0, "");
        short.name = Name {
            given: "Wright".into(),
            family: "M.".into(),
            ..Name::default()
        };
        let full = c(1, "Michael Wright");
        let m = merge(&[&short, &full], Id::IN, &Choices::default());
        assert_eq!(m.contact.display_name(), "Michael Wright");
    }

    #[test]
    fn picks_the_name_most_copies_agree_on() {
        let mut swapped = c(0, "Williams Patricia");
        swapped.name = Name {
            given: "Williams".into(),
            family: "Patricia".into(),
            ..Name::default()
        };
        let typo = c(1, "Ptricia Williams");
        let right = c(2, "Patricia Williams");
        let nick = c(3, "Pat Williams");
        let m = merge(
            &[&swapped, &typo, &right, &nick],
            Id::IN,
            &Choices::default(),
        );
        assert_eq!(m.contact.display_name(), "Patricia Williams");
        // Pat is a nickname; the typo and the swapped surname are not.
        assert_eq!(m.contact.nicknames, ["Pat"]);
    }

    #[test]
    fn prefers_normal_capitals() {
        let (a, b) = (c(0, "KAVITA DUTTA"), c(1, "Kavita Dutta"));
        let m = merge(&[&a, &b], Id::IN, &Choices::default());
        assert_eq!(m.contact.display_name(), "Kavita Dutta");
        // Same name either way, offered once, in its better spelling.
        assert_eq!(m.alternatives.name.len(), 1);
        assert_eq!(m.alternatives.name[0].value, "Kavita Dutta");
    }

    #[test]
    fn prefers_the_name_without_a_title() {
        let (a, b) = (c(0, "Mrs. Kavita Dutta"), c(1, "Kavita Dutta"));
        let m = merge(&[&a, &b], Id::IN, &Choices::default());
        assert_eq!(m.contact.display_name(), "Kavita Dutta");
    }

    #[test]
    fn notes_combine_without_repeating() {
        let n = merge_notes(
            [
                "Likes sailing.",
                "likes   SAILING.\n\nMet at the 2019 conference.",
                "Met at the 2019 conference. Owes me a book.",
            ]
            .into_iter(),
        );
        assert_eq!(
            n,
            "Likes sailing.\n\nMet at the 2019 conference. Owes me a book."
        );
    }

    #[test]
    fn grouped_extras_are_renamed_on_clash() {
        let mut a = c(0, "A");
        let mut b = c(1, "A");
        let ex = |g: &str, n: &str, v: &str| Extra {
            group: Some(g.into()),
            name: n.into(),
            params: vec![],
            value: v.into(),
        };
        a.extra = vec![
            ex("item1", "X-ABDATE", "2015-06-20"),
            ex("item1", "X-ABLABEL", "Anniversary"),
        ];
        b.extra = vec![
            ex("item1", "X-ABDATE", "2015-06-20"),
            ex("item1", "X-ABLABEL", "Anniversary"),
            ex("item2", "X-ABDATE", "2001-01-01"),
        ];
        let mut b2 = b.clone();
        b2.extra[2].group = Some("item1".into());
        b2.extra.truncate(1);
        b2.extra[0].value = "1999-09-09".into();
        let out = merge(&[&a, &b, &b2], Id::IN, &Choices::default())
            .contact
            .extra;
        let groups: Vec<_> = out
            .iter()
            .map(|e| (e.group.clone().unwrap(), e.value.clone()))
            .collect();
        assert_eq!(
            groups,
            [
                ("item1".into(), "2015-06-20".into()),
                ("item1".into(), "Anniversary".into()),
                ("item2".into(), "2001-01-01".into()),
                ("cm1".into(), "1999-09-09".into()),
            ]
        );
    }

    #[test]
    fn apply_merges_in_place_and_drops_empties() {
        let contacts = vec![
            c(0, "Ann"),
            c(1, "Bo"),
            Contact {
                id: 2,
                ..Contact::default()
            },
            c(3, "Ann"),
        ];
        let (out, s) = apply(&contacts, &[(vec![0, 3], Choices::default())], Id::IN);
        let names: Vec<_> = out.iter().map(|c| c.formatted_name.as_str()).collect();
        assert_eq!(names, ["Ann", "Bo"]);
        assert_eq!(
            (
                s.contacts_in,
                s.contacts_out,
                s.groups_merged,
                s.empty_dropped
            ),
            (4, 2, 1, 1)
        );
    }
}
