//! Normalised copies of values, used only for matching. Nothing here is
//! written to the output file.

use phonenumber::country::Id;
use phonenumber::{Mode, PhoneNumber};
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

/// A two-letter region code (`IN`, `GB`), case-insensitive.
pub fn region(code: &str) -> Option<Id> {
    code.trim().to_ascii_uppercase().parse().ok()
}

/// The region to assume for numbers saved without a country code, from the
/// locale (`en_IN.UTF-8` gives IN). Falls back to US.
pub fn default_region() -> Id {
    ["LC_ALL", "LC_TELEPHONE", "LANG"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .find_map(|v| {
            let country = v.split(['.', '@']).next()?.split('_').nth(1)?.to_string();
            region(&country)
        })
        .unwrap_or(Id::US)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Phone {
    /// E.164 (`+919876543210`) when the number parses; otherwise `#` and
    /// its digits, so two copies of an odd number still match each other.
    pub key: String,
    /// How to show it in an explanation: `+91 98765 43210`.
    pub display: String,
    /// The number as saved includes a country code.
    pub has_country: bool,
}

pub fn phone(raw: &str, region: Id) -> Option<Phone> {
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    // Short codes (*121, 911, 100) aren't anyone's number.
    if digits.len() < 6 {
        return None;
    }
    let t = raw.trim();
    let intl = if let Some(rest) = t.strip_prefix("00") {
        Some(format!("+{rest}"))
    } else if t.starts_with('+') {
        Some(t.to_string())
    } else {
        None
    };
    let has_country = intl.is_some();
    let parsed: Option<PhoneNumber> = match &intl {
        Some(s) => phonenumber::parse(None, s).ok(),
        None => phonenumber::parse(Some(region), t).ok(),
    };
    Some(match parsed {
        Some(n) => Phone {
            key: n.format().mode(Mode::E164).to_string(),
            display: n.format().mode(Mode::International).to_string(),
            has_country,
        },
        None => Phone {
            key: format!("#{digits}"),
            display: t.to_string(),
            has_country,
        },
    })
}

/// Lowercased. For Gmail only, dots and anything after `+` in the local
/// part are dropped, because Gmail itself ignores them; other providers
/// may treat them as different mailboxes.
pub fn email(raw: &str) -> Option<String> {
    let e = raw.trim().trim_start_matches("mailto:").to_lowercase();
    let (local, domain) = e.rsplit_once('@')?;
    if local.is_empty() || !domain.contains('.') {
        return None;
    }
    if domain == "gmail.com" || domain == "googlemail.com" {
        let local = local.split('+').next().unwrap_or(local).replace('.', "");
        return Some(format!("{local}@gmail.com"));
    }
    Some(e)
}

/// NFKC, lowercase, accents removed, punctuation to spaces, whitespace
/// collapsed. "Ｊosé  O'Brien-Núñez" becomes "jose obrien nunez".
pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.nfkc().collect::<String>().to_lowercase().nfd() {
        if is_combining_mark(c) {
            continue;
        }
        match c {
            'ß' => out.push_str("ss"),
            'æ' => out.push_str("ae"),
            'œ' => out.push_str("oe"),
            'ø' => out.push('o'),
            'ł' => out.push('l'),
            'đ' | 'ð' => out.push('d'),
            'þ' => out.push_str("th"),
            'ı' => out.push('i'),
            // Apostrophes join: O'Brien and OBrien are the same name.
            '\'' | '’' | '`' => {}
            c if c.is_alphanumeric() => out.push(c),
            _ => out.push(' '),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

const HONORIFICS: &[&str] = &[
    "mr", "mrs", "ms", "miss", "mx", "dr", "prof", "sir", "dame", "rev", "fr", "smt", "shri",
    "sri", "herr", "frau", "sr", "sra", "srta", "mme", "mlle",
];
const SUFFIXES: &[&str] = &[
    "jr", "sr", "ii", "iii", "iv", "phd", "md", "esq", "dds", "cpa", "mba",
];
/// Words people add to tell copies apart ("Bob Smith work", "Mom mobile").
const ANNOTATIONS: &[&str] = &[
    "work",
    "office",
    "home",
    "personal",
    "private",
    "mobile",
    "cell",
    "old",
    "new",
    "landline",
    "business",
    "biz",
    "family",
    "house",
    "residence",
    "main",
    "other",
    "2",
    "3",
    "copy",
];
/// What people call relatives instead of their names.
const ROLES: &[&str] = &[
    "mom", "mum", "mommy", "mummy", "mother", "mama", "maa", "amma", "ammi", "dad", "daddy",
    "father", "papa", "pappa", "appa", "abba", "grandma", "grandpa", "granny", "gran", "nana",
    "nani", "dadi", "dada", "nanu", "wife", "hubby", "husband", "bro", "sis", "my", "aunty",
    "uncle", "auntie",
];
const COMPANY_WORDS: &[&str] = &[
    "inc",
    "ltd",
    "llc",
    "llp",
    "pvt",
    "private",
    "limited",
    "gmbh",
    "co",
    "corp",
    "corporation",
    "company",
    "the",
    "ag",
    "plc",
    "sa",
    "srl",
    "bv",
];

/// Nicknames to the full names they're short for. Kept small on purpose:
/// every entry is a pair a person would agree is the same name.
const NICKNAMES: &[(&str, &[&str])] = &[
    ("abby", &["abigail"]),
    ("alex", &["alexander", "alexandra"]),
    ("andy", &["andrew"]),
    ("drew", &["andrew"]),
    ("ben", &["benjamin"]),
    ("beth", &["elizabeth"]),
    ("betty", &["elizabeth"]),
    ("bill", &["william"]),
    ("billy", &["william"]),
    ("will", &["william"]),
    ("bob", &["robert"]),
    ("bobby", &["robert"]),
    ("rob", &["robert"]),
    ("robbie", &["robert"]),
    ("chris", &["christopher", "christine", "christina"]),
    ("dan", &["daniel"]),
    ("danny", &["daniel"]),
    ("dave", &["david"]),
    ("debbie", &["deborah"]),
    ("ed", &["edward"]),
    ("eddie", &["edward"]),
    ("ted", &["edward", "theodore"]),
    ("greg", &["gregory"]),
    ("jack", &["john"]),
    ("johnny", &["john"]),
    ("jim", &["james"]),
    ("jimmy", &["james"]),
    ("jamie", &["james"]),
    ("jen", &["jennifer"]),
    ("jenny", &["jennifer"]),
    ("joe", &["joseph"]),
    ("kate", &["katherine", "catherine", "kathryn"]),
    ("katie", &["katherine", "catherine", "kathryn"]),
    ("kathy", &["katherine", "catherine", "kathryn"]),
    ("liz", &["elizabeth"]),
    ("lizzie", &["elizabeth"]),
    ("maggie", &["margaret"]),
    ("meg", &["margaret"]),
    ("peggy", &["margaret"]),
    ("matt", &["matthew"]),
    ("mike", &["michael"]),
    ("mick", &["michael"]),
    ("nick", &["nicholas"]),
    ("pat", &["patrick", "patricia"]),
    ("rick", &["richard"]),
    ("rich", &["richard"]),
    ("dick", &["richard"]),
    ("sam", &["samuel", "samantha"]),
    ("steve", &["steven", "stephen"]),
    ("sue", &["susan"]),
    ("susie", &["susan"]),
    ("tom", &["thomas"]),
    ("tommy", &["thomas"]),
    ("tony", &["anthony"]),
    ("tim", &["timothy"]),
    ("vicky", &["victoria"]),
    ("sepp", &["josef"]),
    ("jupp", &["josef"]),
    ("pepe", &["jose"]),
    ("paco", &["francisco"]),
];

/// The name itself plus the full names it may be short for, and the full
/// name for a nickname used as a key.
pub fn canonical(given: &str) -> Vec<&str> {
    let mut out = vec![given];
    for (nick, full) in NICKNAMES {
        if *nick == given {
            out.extend(full.iter());
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameKind {
    Person,
    Company,
    /// "Mom", "Dad", "Nani": a relation, not a name.
    Role,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameKey {
    pub kind: NameKind,
    /// First given token, folded: "robert".
    pub given: String,
    /// Family name, folded, may be several tokens: "garcia lopez".
    pub family: String,
    /// Folded organisation without "Inc", "Ltd" and the like.
    pub org: String,
}

/// Drop bracketed notes: "Bob Smith (work)", "Ann [old]".
fn strip_brackets(s: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in s.chars() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = (depth - 1).max(0),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

fn tokens(s: &str) -> Vec<String> {
    fold(&strip_brackets(s))
        .split(' ')
        .filter(|t| !t.is_empty())
        .map(String::from)
        .collect()
}

fn clean_person(mut t: Vec<String>) -> Vec<String> {
    while t.len() > 1 && HONORIFICS.contains(&t[0].as_str()) {
        t.remove(0);
    }
    while t.len() > 1 && SUFFIXES.contains(&t[t.len() - 1].as_str()) {
        t.pop();
    }
    let kept: Vec<String> = t
        .iter()
        .filter(|w| !ANNOTATIONS.contains(&w.as_str()))
        .cloned()
        .collect();
    if kept.is_empty() { t } else { kept }
}

pub fn org_key(org: &str) -> String {
    tokens(org)
        .into_iter()
        .filter(|t| !COMPANY_WORDS.contains(&t.as_str()))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn name_key(c: &crate::contact::Contact) -> NameKey {
    let org = org_key(&c.org);
    let n = &c.name;
    // N when it has a given or family name; FN otherwise.
    let (given, family) = if !n.given.trim().is_empty() || !n.family.trim().is_empty() {
        let g = clean_person(tokens(&n.given));
        let f = clean_person(tokens(&n.family));
        // N:;Robert Smith;;; puts the whole name in one part.
        if f.is_empty() && g.len() > 1 {
            split_tokens(g)
        } else if g.is_empty() && f.len() > 1 {
            split_tokens(f)
        } else {
            (g.first().cloned().unwrap_or_default(), f.join(" "))
        }
    } else {
        let fname = c.formatted_name.trim();
        if let Some((fam, giv)) = fname.split_once(',') {
            // "Smith, Robert"
            let g = clean_person(tokens(giv));
            let f = clean_person(tokens(fam));
            (g.first().cloned().unwrap_or_default(), f.join(" "))
        } else {
            split_tokens(clean_person(tokens(fname)))
        }
    };
    let all: Vec<&str> = given
        .split(' ')
        .chain(family.split(' '))
        .filter(|t| !t.is_empty())
        .collect();
    let kind = if all.is_empty() {
        if org.is_empty() {
            NameKind::None
        } else {
            NameKind::Company
        }
    } else if all.iter().all(|t| ROLES.contains(t)) {
        NameKind::Role
    } else if !org.is_empty() && org_key(&format!("{given} {family}")) == org {
        // iCloud company cards repeat the company as the name.
        NameKind::Company
    } else if all.iter().all(|t| t.chars().count() == 1) {
        // "J." alone says nothing.
        NameKind::None
    } else {
        NameKind::Person
    };
    NameKey {
        kind,
        given,
        family,
        org,
    }
}

/// "robert james smith" -> given "robert", family "james smith". The
/// family keeps every later word because a middle name and the first of
/// two surnames look the same; matching on any shared family word covers
/// both.
fn split_tokens(t: Vec<String>) -> (String, String) {
    match t.split_first() {
        None => (String::new(), String::new()),
        Some((g, rest)) => (g.clone(), rest.join(" ")),
    }
}

/// How two names relate, from strongest to weakest evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NameMatch {
    Exact,
    Nickname,
    Swapped,
    Typo,
    Initial,
    /// Only one part to compare, and it agrees: "Priya" and "Priya Shah".
    Partial,
    /// Same given name, different family name: marriage, or two people.
    FamilyDiffers,
    /// Nothing to compare (no name, a relation like "Mom", a company).
    Unknown,
    /// Clearly two different people.
    Different,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Given {
    Equal,
    Nick,
    Initial,
    Typo,
    No,
}

fn typo(a: &str, b: &str) -> bool {
    let (la, lb) = (a.chars().count(), b.chars().count());
    if la.min(lb) < 5 || a.chars().next() != b.chars().next() {
        return false;
    }
    // A different last letter is usually a different name, not a slip:
    // Daniel/Daniela, Francesco/Francesca.
    if a.chars().last() != b.chars().last() {
        return false;
    }
    let d = strsim::damerau_levenshtein(a, b);
    d == 1 || (d == 2 && la.min(lb) >= 8)
}

fn given_rel(a: &str, b: &str) -> Given {
    if a.is_empty() || b.is_empty() {
        return Given::No;
    }
    if a == b {
        return Given::Equal;
    }
    let (ca, cb) = (canonical(a), canonical(b));
    if ca.iter().any(|x| cb.contains(x)) {
        return Given::Nick;
    }
    let initial = |short: &str, long: &[&str]| {
        short.chars().count() == 1 && long.iter().any(|l| l.starts_with(short))
    };
    if initial(a, &cb) || initial(b, &ca) {
        return Given::Initial;
    }
    if typo(a, b) {
        return Given::Typo;
    }
    Given::No
}

fn family_rel(a: &str, b: &str) -> Given {
    if a == b {
        return Given::Equal;
    }
    // Double surnames: "garcia lopez" and "garcia".
    let (ta, tb): (Vec<&str>, Vec<&str>) = (a.split(' ').collect(), b.split(' ').collect());
    if ta.iter().any(|t| t.len() > 1 && tb.contains(t)) {
        return Given::Equal;
    }
    if typo(a, b) { Given::Typo } else { Given::No }
}

pub fn compare(a: &NameKey, b: &NameKey) -> NameMatch {
    use NameKind::*;
    match (a.kind, b.kind) {
        (Company, Company) => {
            if a.org == b.org && !a.org.is_empty() {
                NameMatch::Exact
            } else if typo(&a.org, &b.org) {
                NameMatch::Typo
            } else {
                NameMatch::Different
            }
        }
        (Person, Person) => person(a, b),
        _ => NameMatch::Unknown,
    }
}

fn person(a: &NameKey, b: &NameKey) -> NameMatch {
    use NameMatch::*;
    let (af, bf) = (!a.family.is_empty(), !b.family.is_empty());
    if af && bf {
        let fam = family_rel(&a.family, &b.family);
        if fam != Given::No {
            return match (fam, given_rel(&a.given, &b.given)) {
                (Given::Equal, Given::Equal) => Exact,
                (Given::Equal, Given::Nick) => Nickname,
                (Given::Equal, Given::Initial) => Initial,
                (Given::Equal, Given::Typo) | (Given::Typo, Given::Equal) => Typo,
                (Given::Typo, Given::Nick) => Typo,
                _ => Different,
            };
        }
        let cross = (
            given_rel(&a.given, &b.family),
            family_rel(&a.family, &b.given),
        );
        if matches!(cross.0, Given::Equal | Given::Typo)
            && matches!(cross.1, Given::Equal | Given::Typo)
        {
            return Swapped;
        }
        return match given_rel(&a.given, &b.given) {
            Given::Equal | Given::Nick => FamilyDiffers,
            _ => Different,
        };
    }
    // One or both are a single word.
    let (one, other) = if af { (b, a) } else { (a, b) };
    let word = &one.given;
    match given_rel(word, &other.given) {
        Given::Equal | Given::Nick => return Partial,
        Given::Typo if other.family.is_empty() => return Partial,
        _ => {}
    }
    if !other.family.is_empty() && family_rel(word, &other.family) == Given::Equal {
        return Partial;
    }
    Different
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contact::{Contact, Name};

    fn fname(s: &str) -> NameKey {
        name_key(&Contact {
            formatted_name: s.into(),
            ..Contact::default()
        })
    }

    #[test]
    fn phones_to_e164() {
        let p = phone("098765 43210", Id::IN).unwrap();
        assert_eq!(p.key, "+919876543210");
        assert_eq!(p.display, "+91 98765 43210");
        assert!(!p.has_country);
        assert_eq!(
            phone("+91 98765-43210", Id::US).unwrap().key,
            "+919876543210"
        );
        assert_eq!(
            phone("0091 9876543210", Id::US).unwrap().key,
            "+919876543210"
        );
        assert_eq!(phone("(415) 555-2671", Id::US).unwrap().key, "+14155552671");
        assert_eq!(
            phone("+1 415 555 2671", Id::IN).unwrap().key,
            "+14155552671"
        );
        assert_eq!(phone("07911 123456", Id::GB).unwrap().key, "+447911123456");
        assert_eq!(
            phone("0151 23456789", Id::DE).unwrap().key,
            "+4915123456789"
        );
        assert!(phone("+44 7911 123456", Id::IN).unwrap().has_country);
    }

    #[test]
    fn short_codes_and_junk_are_not_numbers() {
        assert!(phone("*121#", Id::IN).is_none());
        assert!(phone("100", Id::IN).is_none());
        assert!(phone("ask reception", Id::IN).is_none());
    }

    #[test]
    fn emails() {
        assert_eq!(
            email(" Rob.Smith+news@GoogleMail.com ").as_deref(),
            Some("robsmith@gmail.com")
        );
        assert_eq!(email("r.o.b@gmail.com").as_deref(), Some("rob@gmail.com"));
        // Dots and plus tags mean something elsewhere.
        assert_eq!(
            email("Rob.Smith+x@Acme.example").as_deref(),
            Some("rob.smith+x@acme.example")
        );
        assert_eq!(email("not an email"), None);
    }

    #[test]
    fn folding() {
        assert_eq!(fold("Ｊosé  O'Brien-Núñez"), "jose obrien nunez");
        assert_eq!(fold("Straße Ærø Łukasz"), "strasse aero lukasz");
    }

    #[test]
    fn name_keys() {
        let k = fname("Dr. Robert J. Smith Jr.");
        assert_eq!((k.given.as_str(), k.family.as_str()), ("robert", "j smith"));
        let k = fname("Smith, Bob (work)");
        assert_eq!((k.given.as_str(), k.family.as_str()), ("bob", "smith"));
        assert_eq!(fname("Mom mobile").kind, NameKind::Role);
        assert_eq!(fname("Nani").kind, NameKind::Role);
        assert_eq!(fname("J.").kind, NameKind::None);
        let c = Contact {
            formatted_name: "Acme Logistics".into(),
            org: "Acme Logistics, Inc.".into(),
            ..Contact::default()
        };
        assert_eq!(name_key(&c).kind, NameKind::Company);
        let c = Contact {
            name: Name {
                given: "Ana María".into(),
                family: "Pérez García".into(),
                ..Name::default()
            },
            ..Contact::default()
        };
        let k = name_key(&c);
        assert_eq!(
            (k.given.as_str(), k.family.as_str()),
            ("ana", "perez garcia")
        );
    }

    #[test]
    fn comparisons() {
        let cmp = |a: &str, b: &str| compare(&fname(a), &fname(b));
        assert_eq!(cmp("Robert Smith", "robert smith"), NameMatch::Exact);
        assert_eq!(cmp("Bob Smith (work)", "Robert Smith"), NameMatch::Nickname);
        assert_eq!(cmp("Liz Taylor", "Beth Taylor"), NameMatch::Nickname);
        assert_eq!(cmp("R. Smith", "Robert Smith"), NameMatch::Initial);
        assert_eq!(cmp("Smith Robert", "Robert Smith"), NameMatch::Swapped);
        assert_eq!(cmp("Jonathan Smith", "Jonathon Smith"), NameMatch::Typo);
        assert_eq!(cmp("José García", "Jose Garcia"), NameMatch::Exact);
        assert_eq!(cmp("Ana Pérez García", "Ana Perez"), NameMatch::Exact);
        assert_eq!(cmp("Priya", "Priya Shah"), NameMatch::Partial);
        assert_eq!(cmp("Priya Shah", "Priya Mehta"), NameMatch::FamilyDiffers);
        assert_eq!(cmp("Priya Shah", "Rahul Shah"), NameMatch::Different);
        assert_eq!(cmp("Mom", "Sunita Shah"), NameMatch::Unknown);
        assert_eq!(cmp("Priya", "Rahul Shah"), NameMatch::Different);
    }

    #[test]
    fn near_names_that_are_different_people() {
        let cmp = |a: &str, b: &str| compare(&fname(a), &fname(b));
        assert_eq!(cmp("Daniel Weber", "Daniela Weber"), NameMatch::Different);
        assert_eq!(cmp("Ravi Shah", "Rani Shah"), NameMatch::Different);
        assert_eq!(
            cmp("Christopher Lee", "Christine Lee"),
            NameMatch::Different
        );
        assert_eq!(cmp("Chris Lee", "Christine Lee"), NameMatch::Nickname);
    }
}
