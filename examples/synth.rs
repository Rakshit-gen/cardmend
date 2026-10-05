//! Build a synthetic address book with known answers.
//!
//!     cargo run --release --example synth -- OUT_DIR [--people 600] [--seed 7]
//!
//! The owner lives in India. Their contacts are spread over four exports
//! the way a real book ends up after years of phones and accounts:
//! icloud.vcf (iPhone, vCard 3.0), google.csv (Google Contacts),
//! android.vcf (an old Android phone, vCard 2.1 with quoted-printable) and
//! outlook.csv (Outlook, Windows-1252). About 4 in 10 people have more
//! than one entry, each copy damaged the way real copies are.
//!
//! Traps that must not be merged are planted on purpose: households that
//! share a landline and address, families with one shared email, office
//! switchboards, and different people with the same common name.
//!
//! truth.json records which person every entry belongs to, what damage each
//! copy got, and the traps. Nothing here reads a real address book.

use std::fmt::Write as _;
use std::path::PathBuf;

use base64::Engine;
use phonenumber::country::Id;
use serde_json::json;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // SplitMix64: tiny, fast, and the same on every machine.
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn chance(&mut self, p: f64) -> bool {
        ((self.next() >> 11) as f64 / (1u64 << 53) as f64) < p
    }
    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len())]
    }
    fn digits(&mut self, n: usize) -> String {
        (0..n)
            .map(|_| char::from(b'0' + self.below(10) as u8))
            .collect()
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Culture {
    Indian,
    English,
    German,
    Spanish,
}

const IN_M: &[&str] = &[
    "Rahul",
    "Amit",
    "Vikram",
    "Arjun",
    "Rohan",
    "Sanjay",
    "Karan",
    "Nikhil",
    "Aditya",
    "Suresh",
    "Rajesh",
    "Anil",
    "Deepak",
    "Manish",
    "Varun",
    "Siddharth",
    "Harish",
    "Prakash",
    "Gaurav",
    "Akash",
];
const IN_F: &[&str] = &[
    "Priya", "Ananya", "Kavya", "Sneha", "Pooja", "Neha", "Divya", "Meera", "Shreya", "Aisha",
    "Sunita", "Anjali", "Ritu", "Swati", "Nisha", "Lakshmi", "Deepa", "Isha", "Kiran", "Tanvi",
];
const IN_LAST: &[&str] = &[
    "Sharma", "Patel", "Shah", "Mehta", "Iyer", "Nair", "Reddy", "Gupta", "Singh", "Kumar",
    "Desai", "Joshi", "Rao", "Verma", "Kapoor", "Menon", "Pillai", "Chopra", "Bhat", "Agarwal",
];
const EN_M: &[&str] = &[
    "Robert",
    "William",
    "James",
    "Michael",
    "Thomas",
    "Daniel",
    "David",
    "Edward",
    "Christopher",
    "Andrew",
    "Matthew",
    "Richard",
    "Joseph",
    "Anthony",
    "Samuel",
    "Benjamin",
    "Nicholas",
    "Steven",
    "Timothy",
    "John",
];
const EN_F: &[&str] = &[
    "Elizabeth",
    "Katherine",
    "Jennifer",
    "Margaret",
    "Susan",
    "Patricia",
    "Victoria",
    "Deborah",
    "Abigail",
    "Samantha",
    "Emily",
    "Sarah",
    "Laura",
    "Rachel",
    "Hannah",
    "Olivia",
    "Charlotte",
    "Sophie",
    "Lucy",
    "Grace",
];
const EN_LAST: &[&str] = &[
    "Smith", "Jones", "Taylor", "Brown", "Williams", "Wilson", "Johnson", "Davies", "Robinson",
    "Wright", "Thompson", "Evans", "Walker", "White", "Roberts", "Green", "Hall", "Wood",
    "Jackson", "Clarke",
];
const DE_M: &[&str] = &[
    "Jürgen",
    "Matthias",
    "Stefan",
    "Andreas",
    "Tobias",
    "Lukas",
    "Jonas",
    "Felix",
    "Sebastian",
    "Florian",
];
const DE_F: &[&str] = &[
    "Anna",
    "Katharina",
    "Julia",
    "Sophie",
    "Lena",
    "Johanna",
    "Miriam",
    "Sabine",
    "Ute",
    "Greta",
];
const DE_LAST: &[&str] = &[
    "Müller",
    "Schmidt",
    "Schneider",
    "Fischer",
    "Weber",
    "Meyer",
    "Wagner",
    "Becker",
    "Schulz",
    "Hoffmann",
    "Köhler",
    "Groß",
];
const ES_M: &[&str] = &[
    "José",
    "Javier",
    "Carlos",
    "Miguel",
    "Alejandro",
    "Pablo",
    "Andrés",
    "Raúl",
];
const ES_F: &[&str] = &[
    "María", "Lucía", "Carmen", "Ana", "Sofía", "Isabel", "Inés", "Elena",
];
const ES_LAST: &[&str] = &[
    "García",
    "López",
    "Martínez",
    "Rodríguez",
    "Pérez",
    "Sánchez",
    "Gómez",
    "Fernández",
    "Núñez",
];

/// Nicknames people actually save, matching cardmend's table on purpose
/// for some and not for others (Indian short forms aren't in it).
const NICKS: &[(&str, &str)] = &[
    ("Robert", "Bob"),
    ("Robert", "Rob"),
    ("William", "Bill"),
    ("William", "Will"),
    ("James", "Jim"),
    ("Michael", "Mike"),
    ("Thomas", "Tom"),
    ("Daniel", "Dan"),
    ("David", "Dave"),
    ("Edward", "Ed"),
    ("Christopher", "Chris"),
    ("Andrew", "Andy"),
    ("Matthew", "Matt"),
    ("Richard", "Rick"),
    ("Joseph", "Joe"),
    ("Anthony", "Tony"),
    ("Samuel", "Sam"),
    ("Benjamin", "Ben"),
    ("Nicholas", "Nick"),
    ("Steven", "Steve"),
    ("Timothy", "Tim"),
    ("Elizabeth", "Liz"),
    ("Elizabeth", "Beth"),
    ("Katherine", "Kate"),
    ("Jennifer", "Jen"),
    ("Margaret", "Maggie"),
    ("Susan", "Sue"),
    ("Patricia", "Pat"),
    ("Victoria", "Vicky"),
    ("Deborah", "Debbie"),
    ("Abigail", "Abby"),
    ("Samantha", "Sam"),
    ("Siddharth", "Sid"),
    ("Aditya", "Adi"),
    ("Lakshmi", "Lakshmi"),
    ("Varun", "Varun"),
];

const COMPANIES: &[(&str, &str)] = &[
    ("Bluefin Analytics", "bluefin.example"),
    ("Kestrel Logistics", "kestrel.example"),
    ("Lakeside Clinic", "lakeside-clinic.example"),
    ("Orbit Software", "orbitsoft.example"),
    ("Saffron Foods", "saffronfoods.example"),
    ("Meridian Bank", "meridian.example"),
    ("Copperleaf Design", "copperleaf.example"),
    ("Northwind Traders", "northwind.example"),
];

const MAIL: &[&str] = &[
    "gmail.com",
    "gmail.com",
    "gmail.com",
    "yahoo.com",
    "outlook.com",
    "hotmail.com",
    "icloud.com",
    "rediffmail.com",
];

#[derive(Clone)]
struct Person {
    id: usize,
    culture: Culture,
    given: String,
    family: String,
    nick: Option<String>,
    /// E.164 without the plus, and its country.
    mobile: (String, Id),
    email: Option<String>,
    work: Option<usize>,
    work_email: Option<String>,
    direct: Option<String>,
    landline: Option<String>,
    family_email: Option<String>,
    address: Option<[String; 4]>,
    birthday: Option<(u16, u8, u8)>,
    note: Option<String>,
    photo: Option<Vec<u8>>,
    /// "Mom" or "Dad" for the owner's parents.
    role: Option<&'static str>,
}

fn valid(e164: &str) -> bool {
    phonenumber::parse(None, format!("+{e164}")).is_ok_and(|n| phonenumber::is_valid(&n))
}

fn mobile(rng: &mut Rng, country: Id) -> String {
    loop {
        let n = match country {
            Id::IN => format!("91{}{}", rng.pick(&["6", "7", "8", "9"]), rng.digits(9)),
            Id::US => format!(
                "1{}{}{}",
                rng.pick(&["212", "415", "646", "312", "617", "206", "512", "303"]),
                2 + rng.below(8),
                rng.digits(6)
            ),
            Id::GB => format!(
                "447{}{}",
                rng.pick(&["4", "5", "7", "8", "9"]),
                rng.digits(8)
            ),
            Id::DE => format!(
                "491{}{}",
                rng.pick(&["51", "52", "57", "60", "70", "76"]),
                rng.digits(8)
            ),
            _ => format!("346{}", rng.digits(8)),
        };
        if valid(&n) {
            return n;
        }
    }
}

fn landline(rng: &mut Rng, area: &str) -> String {
    loop {
        let n = format!("91{area}{}{}", 2 + rng.below(5), rng.digits(9 - area.len()));
        if valid(&n) {
            return n;
        }
    }
}

/// A number written the way a given app or person would.
fn format_number(rng: &mut Rng, e164: &str, country: Id, style: Style) -> String {
    let cc = match country {
        Id::IN => "91",
        Id::US => "1",
        Id::GB => "44",
        Id::DE => "49",
        _ => "34",
    };
    let nat = &e164[cc.len()..];
    let n = phonenumber::parse(None, format!("+{e164}")).unwrap();
    let intl = n
        .format()
        .mode(phonenumber::Mode::International)
        .to_string();
    let national = n.format().mode(phonenumber::Mode::National).to_string();
    match style {
        Style::Intl => intl,
        Style::Typed => {
            // Domestic numbers as people key them in; foreign ones mostly
            // keep their country code, sometimes as 00.
            if country == Id::IN {
                match rng.below(5) {
                    0 => national,
                    4 => format!("+{e164}"),
                    1 => nat.to_string(),
                    2 => format!("0{nat}"),
                    _ => format!("{}-{}", &nat[..5.min(nat.len())], &nat[5.min(nat.len())..]),
                }
            } else if rng.chance(0.08) {
                // Saved without a country code: cardmend will read it as Indian.
                national
            } else if rng.chance(0.3) {
                format!("00{cc} {nat}")
            } else {
                intl.replace(' ', "-")
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Style {
    Intl,
    Typed,
}

fn fake_jpeg(rng: &mut Rng) -> Vec<u8> {
    let len = 2_000 + rng.below(18_000);
    let mut v = vec![0xFF, 0xD8, 0xFF, 0xE0, 0, 16, b'J', b'F', b'I', b'F', 0];
    v.extend((0..len).map(|_| rng.next() as u8));
    v.extend([0xFF, 0xD9]);
    v
}

fn ascii(s: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    s.nfd()
        .filter(|c| c.is_ascii())
        .collect::<String>()
        .replace(['ß'], "ss")
}

fn email_for(rng: &mut Rng, given: &str, family: &str) -> String {
    let (g, f) = (ascii(given).to_lowercase(), ascii(family).to_lowercase());
    let local = match rng.below(5) {
        0 => format!("{g}.{f}"),
        1 => format!("{g}{f}{}", rng.digits(2)),
        2 => format!("{}{f}", &g[..1]),
        3 => format!("{g}_{f}{}", 70 + rng.below(30)),
        _ => format!("{f}.{g}"),
    };
    format!("{local}@{}", rng.pick(MAIL))
}

fn typo(rng: &mut Rng, s: &str) -> String {
    let mut c: Vec<char> = s.chars().collect();
    if c.len() < 4 {
        return s.to_string();
    }
    let i = 1 + rng.below(c.len() - 2);
    match rng.below(3) {
        0 => c.swap(i, i + 1),
        1 => {
            c.remove(i);
        }
        _ => {
            let x = c[i];
            c.insert(i, x);
        }
    }
    c.into_iter().collect()
}

/// One entry as it will be exported: what the copy kept and how it's named.
#[derive(Clone, Default)]
struct Entry {
    person: usize,
    formatted: String,
    given: String,
    family: String,
    prefix: String,
    org: String,
    title: String,
    phones: Vec<(String, &'static str)>,
    emails: Vec<(String, &'static str)>,
    address: Option<[String; 4]>,
    birthday: Option<(u16, u8, u8)>,
    note: String,
    photo: Option<Vec<u8>>,
    damage: Vec<&'static str>,
}

#[derive(Clone, Copy, PartialEq)]
enum Format {
    ICloud,
    Google,
    Android,
    Outlook,
}

fn style_for(f: Format) -> Style {
    match f {
        Format::ICloud | Format::Outlook => Style::Typed,
        Format::Google => Style::Intl,
        Format::Android => Style::Typed,
    }
}

fn base_entry(
    rng: &mut Rng,
    p: &Person,
    f: Format,
    companies: &[(String, String, String)],
) -> Entry {
    let style = style_for(f);
    let mut e = Entry {
        person: p.id,
        given: p.given.clone(),
        family: p.family.clone(),
        ..Entry::default()
    };
    e.formatted = format!("{} {}", p.given, p.family);
    e.phones
        .push((format_number(rng, &p.mobile.0, p.mobile.1, style), "mobile"));
    if let Some(l) = &p.landline {
        e.phones
            .push((format_number(rng, l, Id::IN, style), "home"));
    }
    if let Some(em) = &p.email {
        e.emails.push((em.clone(), "home"));
    }
    if let Some(em) = &p.family_email {
        e.emails.push((em.clone(), "home"));
    }
    if let Some(w) = p.work {
        e.org = companies[w].0.clone();
        if rng.chance(0.5) {
            e.title = rng
                .pick(&[
                    "Engineer",
                    "Manager",
                    "Analyst",
                    "Designer",
                    "Doctor",
                    "Accountant",
                    "Director",
                ])
                .to_string();
        }
    }
    e.address = p.address.clone();
    e.birthday = p.birthday;
    e.note = p.note.clone().unwrap_or_default();
    e.photo = p.photo.clone();
    e
}

fn damage(
    rng: &mut Rng,
    p: &Person,
    mut e: Entry,
    f: Format,
    companies: &[(String, String, String)],
) -> Entry {
    let style = style_for(f);
    // Each copy gets one to three kinds of damage.
    let kinds = 1 + rng.below(3);
    for _ in 0..kinds {
        match rng.below(12) {
            0 | 1 => {
                if let Some(n) = &p.nick {
                    e.given = n.clone();
                    e.formatted = format!("{} {}", n, e.family);
                    e.damage.push("nickname");
                } else {
                    e.given = format!(
                        "{}.",
                        &e.given[..e.given.char_indices().nth(1).map_or(e.given.len(), |x| x.0)]
                    );
                    e.formatted = format!("{} {}", e.given, e.family);
                    e.damage.push("initial");
                }
            }
            2 => {
                std::mem::swap(&mut e.given, &mut e.family);
                e.formatted = format!("{} {}", e.given, e.family);
                e.damage.push("swapped");
            }
            3 => {
                if rng.chance(0.5) {
                    e.family = typo(rng, &e.family);
                } else {
                    e.given = typo(rng, &e.given);
                }
                e.formatted = format!("{} {}", e.given, e.family);
                e.damage.push("typo");
            }
            4 if e.emails.len() + e.phones.len() > 1 => {
                if rng.chance(0.5) && !e.emails.is_empty() {
                    e.emails.clear();
                } else {
                    e.phones.truncate(0);
                }
                e.damage.push("missing_field");
            }
            5 => {
                if let Some(w) = p.work {
                    // The work copy: work details, sometimes the mobile too.
                    e.formatted = format!("{} {} (work)", e.given, e.family);
                    e.family = format!("{} (work)", e.family);
                    if !rng.chance(0.5) {
                        e.phones.clear();
                    }
                    e.emails.clear();
                    if let Some(d) = &p.direct {
                        e.phones
                            .push((format_number(rng, d, Id::IN, style), "work"));
                    }
                    e.phones
                        .push((format_number(rng, &companies[w].2, Id::IN, style), "main"));
                    if let Some(we) = &p.work_email {
                        e.emails.push((we.clone(), "work"));
                    }
                    e.org = companies[w].0.clone();
                    e.address = None;
                    e.damage.push("work_copy");
                }
            }
            6 => {
                e.formatted = if rng.chance(0.5) {
                    e.formatted.to_uppercase()
                } else {
                    e.formatted.to_lowercase()
                };
                e.given = String::new();
                e.family = String::new();
                e.damage.push("case");
            }
            7 => {
                e.given = ascii(&e.given);
                e.family = ascii(&e.family);
                e.formatted = ascii(&e.formatted);
                e.damage.push("accents");
            }
            8 => {
                // Saved in a hurry: first name and mobile only.
                e.family = String::new();
                e.formatted = e.given.clone();
                e.phones.truncate(1);
                e.emails.clear();
                e.org.clear();
                e.address = None;
                e.note.clear();
                e.damage.push("first_name_only");
            }
            9 => {
                e.birthday = None;
                e.address = None;
                e.photo = None;
                e.damage.push("sparse");
            }
            10 => {
                e.prefix = rng.pick(&["Dr.", "Mr.", "Mrs.", "Ms."]).to_string();
                e.formatted = format!("{} {}", e.prefix, e.formatted);
                e.damage.push("honorific");
            }
            _ => {
                if rng.chance(0.25) {
                    // A number with no name at all.
                    e.given.clear();
                    e.family.clear();
                    e.formatted.clear();
                    e.prefix.clear();
                    e.emails.clear();
                    e.org.clear();
                    e.phones.truncate(1);
                    e.address = None;
                    e.birthday = None;
                    e.note.clear();
                    e.photo = None;
                    e.damage.push("number_only");
                    break;
                }
                e.note = match e.note.is_empty() {
                    true => "Old number, check before calling".into(),
                    false => format!("{}\n\nAdded from old phone", e.note),
                };
                e.damage.push("note");
            }
        }
    }
    if let Some(ph) = e.phones.first_mut() {
        ph.0 = format_number(rng, &p.mobile.0, p.mobile.1, style);
    }
    e
}

// ---- writers, one per exporter ----

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace(',', "\\,")
        .replace(';', "\\;")
        .replace('\n', "\\n")
}

fn fold(line: &str, out: &mut String) {
    let mut w = 0;
    for ch in line.chars() {
        if w + ch.len_utf8() > 75 {
            out.push_str("\r\n ");
            w = 1;
        }
        out.push(ch);
        w += ch.len_utf8();
    }
    out.push_str("\r\n");
}

fn icloud(entries: &[Entry]) -> String {
    let mut out = String::new();
    for e in entries {
        let mut l: Vec<String> = vec![
            "BEGIN:VCARD".into(),
            "VERSION:3.0".into(),
            "PRODID:-//Apple Inc.//iPhone OS 17.5//EN".into(),
        ];
        l.push(format!(
            "N:{};{};;{};",
            esc(&e.family),
            esc(&e.given),
            esc(&e.prefix)
        ));
        let fname = if e.formatted.is_empty() {
            e.org.clone()
        } else {
            e.formatted.clone()
        };
        l.push(format!("FN:{}", esc(&fname)));
        if !e.org.is_empty() {
            l.push(format!("ORG:{};", esc(&e.org)));
        }
        if !e.title.is_empty() {
            l.push(format!("TITLE:{}", esc(&e.title)));
        }
        let mut item = 0;
        for (v, kind) in &e.emails {
            item += 1;
            l.push(format!(
                "item{item}.EMAIL;type=INTERNET;type={}:{v}",
                if *kind == "work" { "WORK" } else { "HOME" }
            ));
        }
        for (v, kind) in &e.phones {
            match *kind {
                "mobile" => l.push(format!("TEL;type=CELL;type=VOICE;type=pref:{v}")),
                "home" => l.push(format!("TEL;type=HOME;type=VOICE:{v}")),
                "work" => l.push(format!("TEL;type=WORK;type=VOICE:{v}")),
                _ => {
                    item += 1;
                    l.push(format!("item{item}.TEL:{v}"));
                    l.push(format!("item{item}.X-ABLabel:_$!<Main>!$_"));
                }
            }
        }
        if let Some(a) = &e.address {
            item += 1;
            l.push(format!(
                "item{item}.ADR;type=HOME;type=pref:;;{};{};{};{};India",
                esc(&a[0]),
                esc(&a[1]),
                esc(&a[2]),
                esc(&a[3])
            ));
            l.push(format!("item{item}.X-ABADR:in"));
        }
        match e.birthday {
            Some((0, m, d)) => l.push(format!("BDAY;X-APPLE-OMIT-YEAR=1604:1604-{m:02}-{d:02}")),
            Some((y, m, d)) => l.push(format!("BDAY:{y}-{m:02}-{d:02}")),
            None => {}
        }
        if !e.note.is_empty() {
            l.push(format!("NOTE:{}", esc(&e.note)));
        }
        if let Some(p) = &e.photo {
            l.push(format!(
                "PHOTO;ENCODING=b;TYPE=JPEG:{}",
                base64::engine::general_purpose::STANDARD.encode(p)
            ));
        }
        l.push("END:VCARD".into());
        for line in l {
            fold(&line, &mut out);
        }
    }
    out
}

fn csv_cell(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn google(entries: &[Entry]) -> String {
    let header = "First Name,Middle Name,Last Name,Phonetic First Name,Phonetic Middle Name,Phonetic Last Name,Name Prefix,Name Suffix,Nickname,File As,Organization Name,Organization Title,Organization Department,Birthday,Notes,Photo,Labels,E-mail 1 - Label,E-mail 1 - Value,E-mail 2 - Label,E-mail 2 - Value,Phone 1 - Label,Phone 1 - Value,Phone 2 - Label,Phone 2 - Value,Phone 3 - Label,Phone 3 - Value,Address 1 - Label,Address 1 - Formatted,Address 1 - Street,Address 1 - City,Address 1 - PO Box,Address 1 - Region,Address 1 - Postal Code,Address 1 - Country,Address 1 - Extended Address";
    let mut out = String::from(header);
    out.push('\n');
    for e in entries {
        let (given, family) =
            if e.given.is_empty() && e.family.is_empty() && !e.formatted.is_empty() {
                // Google keeps a typed-in name whole in First Name.
                (e.formatted.clone(), String::new())
            } else {
                (e.given.clone(), e.family.clone())
            };
        let mut row: Vec<String> = vec![
            given,
            String::new(),
            family,
            String::new(),
            String::new(),
            String::new(),
            e.prefix.clone(),
            String::new(),
            String::new(),
            String::new(),
            e.org.clone(),
            e.title.clone(),
            String::new(),
        ];
        row.push(match e.birthday {
            Some((0, m, d)) => format!("--{m:02}-{d:02}"),
            Some((y, m, d)) => format!("{y}-{m:02}-{d:02}"),
            None => String::new(),
        });
        row.push(e.note.clone());
        row.push(if e.photo.is_some() {
            "https://lh3.googleusercontent.com/contacts/synthetic".into()
        } else {
            String::new()
        });
        row.push("* myContacts".into());
        for i in 0..2 {
            match e.emails.get(i) {
                Some((v, k)) => {
                    row.push(if *k == "work" {
                        "Work".into()
                    } else {
                        "* Home".into()
                    });
                    row.push(v.clone());
                }
                None => row.extend([String::new(), String::new()]),
            }
        }
        for i in 0..3 {
            match e.phones.get(i) {
                Some((v, k)) => {
                    row.push(
                        match *k {
                            "mobile" => "Mobile",
                            "home" => "Home",
                            "work" => "Work",
                            _ => "Main",
                        }
                        .into(),
                    );
                    row.push(v.clone());
                }
                None => row.extend([String::new(), String::new()]),
            }
        }
        match &e.address {
            Some(a) => row.extend([
                "Home".to_string(),
                format!("{}\n{} {}\n{}", a[0], a[1], a[3], a[2]),
                a[0].clone(),
                a[1].clone(),
                String::new(),
                a[2].clone(),
                a[3].clone(),
                "India".into(),
                String::new(),
            ]),
            None => row.extend(std::iter::repeat_n(String::new(), 9)),
        }
        out.push_str(
            &row.iter()
                .map(|c| csv_cell(c))
                .collect::<Vec<_>>()
                .join(","),
        );
        out.push('\n');
    }
    out
}

fn qp(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b == b' ' || b == b'.' || b == b'@' {
                (b as char).to_string()
            } else {
                format!("={b:02X}")
            }
        })
        .collect()
}

/// 2.1 lines: non-ASCII values in quoted-printable with soft breaks.
fn android_line(name: &str, value: &str, out: &mut String) {
    if value.is_ascii() && !value.contains('\n') {
        out.push_str(&format!("{name}:{value}\r\n"));
        return;
    }
    let line = format!(
        "{name};CHARSET=UTF-8;ENCODING=QUOTED-PRINTABLE:{}",
        qp(value)
    );
    let mut w = 0;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        // Break before 76 columns, never inside an =XX escape.
        if w >= 70 && c == '=' {
            out.push_str("=\r\n");
            w = 0;
        }
        out.push(c);
        w += 1;
    }
    out.push_str("\r\n");
}

fn android(rng: &mut Rng, entries: &[Entry]) -> String {
    let mut out = String::new();
    for e in entries {
        out.push_str("BEGIN:VCARD\r\nVERSION:2.1\r\n");
        if e.person == usize::MAX {
            // An empty entry, as old phones leave behind.
            out.push_str("N:;;;;\r\nEND:VCARD\r\n");
            continue;
        }
        android_line(
            "N",
            &format!("{};{};;{};", e.family, e.given, e.prefix),
            &mut out,
        );
        if !e.formatted.is_empty() {
            android_line("FN", &e.formatted, &mut out);
        }
        for (v, kind) in &e.phones {
            let t = match *kind {
                "mobile" => "CELL",
                "home" => "HOME",
                "work" => "WORK",
                _ => "X-Main",
            };
            out.push_str(&format!("TEL;{t}:{v}\r\n"));
        }
        for (v, kind) in &e.emails {
            out.push_str(&format!(
                "EMAIL;{}:{v}\r\n",
                if *kind == "work" { "WORK" } else { "HOME" }
            ));
        }
        if !e.org.is_empty() {
            android_line("ORG", &e.org, &mut out);
        }
        if !e.title.is_empty() {
            android_line("TITLE", &e.title, &mut out);
        }
        if let Some(a) = &e.address {
            android_line(
                "ADR;HOME",
                &format!(";;{};{};{};{};India", a[0], a[1], a[2], a[3]),
                &mut out,
            );
        }
        if let Some((y, m, d)) = e.birthday
            && y != 0
        {
            out.push_str(&format!("BDAY:{y}-{m:02}-{d:02}\r\n"));
        }
        if !e.note.is_empty() {
            android_line("NOTE", &e.note, &mut out);
        }
        if let Some(p) = &e.photo {
            let b = base64::engine::general_purpose::STANDARD.encode(p);
            out.push_str("PHOTO;ENCODING=BASE64;JPEG:");
            let mut first = true;
            for chunk in b.as_bytes().chunks(72) {
                if !first {
                    out.push_str("  ");
                }
                out.push_str(std::str::from_utf8(chunk).unwrap());
                out.push_str("\r\n");
                first = false;
            }
            out.push_str("\r\n");
        }
        if rng.chance(0.1) {
            out.push_str("X-ANDROID-CUSTOM:vnd.android.cursor.item/contact_event;2010-05-01;1;;;;;;;;;;;;;\r\n");
        }
        out.push_str("END:VCARD\r\n");
    }
    out
}

fn outlook(entries: &[Entry]) -> Vec<u8> {
    let cols = [
        "First Name",
        "Middle Name",
        "Last Name",
        "Title",
        "Suffix",
        "Company",
        "Department",
        "Job Title",
        "Business Street",
        "Business City",
        "Business State",
        "Business Postal Code",
        "Business Country/Region",
        "Home Street",
        "Home City",
        "Home State",
        "Home Postal Code",
        "Home Country/Region",
        "Business Phone",
        "Company Main Phone",
        "Home Phone",
        "Mobile Phone",
        "Other Phone",
        "Birthday",
        "Categories",
        "E-mail Address",
        "E-mail Display Name",
        "E-mail 2 Address",
        "Notes",
    ];
    let mut out = cols
        .iter()
        .map(|c| format!("\"{c}\""))
        .collect::<Vec<_>>()
        .join(",");
    out.push_str("\r\n");
    for e in entries {
        let mut row = vec![String::new(); cols.len()];
        let set = |row: &mut Vec<String>, col: &str, v: String| {
            let i = cols.iter().position(|c| *c == col).unwrap();
            row[i] = v;
        };
        if e.given.is_empty() && e.family.is_empty() {
            set(&mut row, "First Name", e.formatted.clone());
        } else {
            set(&mut row, "First Name", e.given.clone());
            set(&mut row, "Last Name", e.family.clone());
        }
        set(&mut row, "Title", e.prefix.clone());
        set(&mut row, "Company", e.org.clone());
        set(&mut row, "Job Title", e.title.clone());
        for (v, kind) in &e.phones {
            let col = match *kind {
                "mobile" => "Mobile Phone",
                "home" => "Home Phone",
                "work" => "Business Phone",
                _ => "Company Main Phone",
            };
            set(&mut row, col, v.clone());
        }
        for (i, (v, _)) in e.emails.iter().take(2).enumerate() {
            set(
                &mut row,
                if i == 0 {
                    "E-mail Address"
                } else {
                    "E-mail 2 Address"
                },
                v.clone(),
            );
        }
        if let Some(a) = &e.address {
            set(&mut row, "Home Street", a[0].clone());
            set(&mut row, "Home City", a[1].clone());
            set(&mut row, "Home State", a[2].clone());
            set(&mut row, "Home Postal Code", a[3].clone());
            set(&mut row, "Home Country/Region", "India".into());
        }
        set(
            &mut row,
            "Birthday",
            match e.birthday {
                Some((y, m, d)) if y != 0 => format!("{m}/{d}/{y}"),
                _ => "0/0/00".into(),
            },
        );
        set(&mut row, "Notes", e.note.clone());
        out.push_str(
            &row.iter()
                .map(|c| format!("\"{}\"", c.replace('"', "\"\"")))
                .collect::<Vec<_>>()
                .join(","),
        );
        out.push_str("\r\n");
    }
    encoding_rs::WINDOWS_1252.encode(&out).0.into_owned()
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut dir = None;
    let (mut people, mut seed) = (600usize, 7u64);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--people" => people = it.next().and_then(|v| v.parse().ok()).unwrap_or(people),
            "--seed" => seed = it.next().and_then(|v| v.parse().ok()).unwrap_or(seed),
            other => dir = Some(PathBuf::from(other)),
        }
    }
    let dir = dir.ok_or_else(|| anyhow::anyhow!("usage: synth OUT_DIR [--people N] [--seed S]"))?;
    std::fs::create_dir_all(&dir)?;
    let mut rng = Rng(seed);

    // Each company: name, domain, switchboard number (Bengaluru).
    let companies: Vec<(String, String, String)> = COMPANIES
        .iter()
        .map(|(n, d)| (n.to_string(), d.to_string(), landline(&mut rng, "80")))
        .collect();

    let mut persons: Vec<Person> = Vec::new();
    let mut traps: Vec<serde_json::Value> = Vec::new();
    let new_person = |rng: &mut Rng, id: usize, culture: Option<Culture>| -> Person {
        let culture = culture.unwrap_or(match rng.below(100) {
            0..=59 => Culture::Indian,
            60..=84 => Culture::English,
            85..=93 => Culture::German,
            _ => Culture::Spanish,
        });
        let female = rng.chance(0.5);
        let (m, f, l) = match culture {
            Culture::Indian => (IN_M, IN_F, IN_LAST),
            Culture::English => (EN_M, EN_F, EN_LAST),
            Culture::German => (DE_M, DE_F, DE_LAST),
            Culture::Spanish => (ES_M, ES_F, ES_LAST),
        };
        let given = rng.pick(if female { f } else { m }).to_string();
        let family = rng.pick(l).to_string();
        let country = match culture {
            Culture::Indian if rng.chance(0.1) => *rng.pick(&[Id::US, Id::GB]),
            Culture::Indian => Id::IN,
            Culture::English => *rng.pick(&[Id::US, Id::GB, Id::IN]),
            Culture::German => Id::DE,
            Culture::Spanish => Id::ES,
        };
        let nicks: Vec<&str> = NICKS
            .iter()
            .filter(|(g, _)| *g == given)
            .map(|(_, n)| *n)
            .collect();
        let nick = (!nicks.is_empty() && rng.chance(0.7)).then(|| rng.pick(&nicks).to_string());
        let mob = mobile(rng, country);
        let email = rng.chance(0.75).then(|| email_for(rng, &given, &family));
        let birthday = rng.chance(0.4).then(|| {
            let y = if rng.chance(0.7) {
                1950 + rng.below(55) as u16
            } else {
                0
            };
            (y, 1 + rng.below(12) as u8, 1 + rng.below(28) as u8)
        });
        let address = rng.chance(0.3).then(|| {
            let (city, state, pin) = *rng.pick(&[
                ("Mumbai", "MH", "4000"),
                ("Bengaluru", "KA", "5600"),
                ("Pune", "MH", "4110"),
                ("Chennai", "TN", "6000"),
                ("Delhi", "DL", "1100"),
            ]);
            [
                format!(
                    "{} {} Road",
                    1 + rng.below(200),
                    rng.pick(&["MG", "Hill", "Station", "Lake", "Temple", "Park"])
                ),
                city.into(),
                state.into(),
                format!("{pin}{}", rng.digits(2)),
            ]
        });
        let note = rng.chance(0.15).then(|| {
            rng.pick(&[
                "Met at Priya's wedding",
                "School friend",
                "Plumber, good and cheap",
                "Neighbour from the old flat",
                "Ask about the cricket tickets",
                "Dentist reception",
            ])
            .to_string()
        });
        let photo = rng.chance(0.12).then(|| fake_jpeg(rng));
        Person {
            id,
            culture,
            given,
            family,
            nick,
            mobile: (mob, country),
            email,
            work: None,
            work_email: None,
            direct: None,
            landline: None,
            family_email: None,
            address,
            birthday,
            note,
            photo,
            role: None,
        }
    };

    while persons.len() < people {
        let id = persons.len();
        let mut p = new_person(&mut rng, id, None);
        if rng.chance(0.3) {
            let w = rng.below(companies.len());
            p.work = Some(w);
            p.work_email = Some(format!(
                "{}.{}@{}",
                ascii(&p.given).to_lowercase(),
                ascii(&p.family).to_lowercase(),
                companies[w].1
            ));
            p.direct = rng.chance(0.5).then(|| landline(&mut rng, "80"));
        }
        persons.push(p);
        // Households: two to four people, same family name, one landline
        // and address, sometimes one shared email.
        if rng.chance(0.06) && persons.len() + 3 < people {
            let head = persons.last().unwrap().clone();
            let size = 2 + rng.below(3);
            let line = landline(&mut rng, "22");
            let fam_email = rng.chance(0.35).then(|| {
                format!(
                    "{}family{}@gmail.com",
                    ascii(&head.family).to_lowercase(),
                    rng.digits(2)
                )
            });
            let mut ids = vec![head.id];
            persons.last_mut().unwrap().landline = Some(line.clone());
            persons.last_mut().unwrap().family_email = fam_email.clone();
            let mut used = vec![head.given.clone()];
            for _ in 1..size {
                let mut q = new_person(&mut rng, persons.len(), Some(head.culture));
                while used.contains(&q.given) {
                    q = new_person(&mut rng, persons.len(), Some(head.culture));
                }
                used.push(q.given.clone());
                q.family = head.family.clone();
                q.landline = Some(line.clone());
                q.address = head.address.clone();
                q.family_email = fam_email.clone();
                ids.push(q.id);
                persons.push(q);
            }
            traps.push(json!({ "kind": "household", "persons": ids }));
            if fam_email.is_some() {
                traps.push(json!({ "kind": "family_email", "persons": ids }));
            }
        }
        // Two different people with the same common name.
        if rng.chance(0.025) && persons.len() < people {
            let first = persons.last().unwrap().clone();
            let mut twin = new_person(&mut rng, persons.len(), Some(first.culture));
            twin.given = first.given.clone();
            twin.family = first.family.clone();
            twin.nick = first.nick.clone();
            traps.push(json!({ "kind": "same_name", "persons": [first.id, twin.id] }));
            persons.push(twin);
        }
    }
    // Colleagues at a company all carry its switchboard in their work copy.
    for (w, _) in companies.iter().enumerate() {
        let ids: Vec<usize> = persons
            .iter()
            .filter(|p| p.work == Some(w))
            .map(|p| p.id)
            .collect();
        if ids.len() > 1 {
            traps.push(json!({ "kind": "switchboard", "persons": ids }));
        }
    }
    // The owner's parents, saved as Mom and Dad on the phone.
    let parents_line = landline(&mut rng, "22");
    for (i, role) in ["Mom", "Dad"].into_iter().enumerate() {
        let id = persons.len();
        let mut p = new_person(&mut rng, id, Some(Culture::Indian));
        p.given = if i == 0 { "Sunita" } else { "Ramesh" }.into();
        p.family = "Agarwal".into();
        p.nick = None;
        p.landline = Some(parents_line.clone());
        p.role = Some(role);
        persons.push(p);
    }
    let n = persons.len();
    traps.push(json!({ "kind": "household", "persons": [n - 2, n - 1] }));

    let mut by_format: Vec<(Format, Vec<Entry>)> = vec![
        (Format::ICloud, Vec::new()),
        (Format::Google, Vec::new()),
        (Format::Android, Vec::new()),
        (Format::Outlook, Vec::new()),
    ];
    let pick_format = |rng: &mut Rng| match rng.below(100) {
        0..=44 => 0,
        45..=69 => 1,
        70..=89 => 2,
        _ => 3,
    };
    for p in &persons {
        if let Some(role) = p.role {
            // Mom on the iPhone, "Mummy" on the old Android, full name in Google.
            let mut e = base_entry(&mut rng, p, Format::ICloud, &companies);
            e.given = role.into();
            e.family.clear();
            e.formatted = role.into();
            e.damage.push("role_name");
            by_format[0].1.push(e);
            let mut e = base_entry(&mut rng, p, Format::Android, &companies);
            e.given = if role == "Mom" { "Mummy" } else { "Papa" }.into();
            e.family.clear();
            e.formatted = e.given.clone();
            e.damage.push("role_name");
            by_format[2].1.push(e);
            let e = base_entry(&mut rng, p, Format::Google, &companies);
            by_format[1].1.push(e);
            continue;
        }
        let first = pick_format(&mut rng);
        let e = base_entry(&mut rng, p, by_format[first].0, &companies);
        by_format[first].1.push(e);
        let extra = match rng.below(100) {
            0..=29 => 1,
            30..=37 => 2,
            38..=39 => 3,
            _ => 0,
        };
        for _ in 0..extra {
            let mut f = pick_format(&mut rng);
            if f == first && rng.chance(0.7) {
                f = (f + 1 + rng.below(3)) % 4;
            }
            let fmt = by_format[f].0;
            let e = base_entry(&mut rng, p, fmt, &companies);
            let e = damage(&mut rng, p, e, fmt, &companies);
            by_format[f].1.push(e);
        }
    }
    // Shuffle each file so copies aren't next to each other, and add a few
    // empty entries to the Android export.
    for (f, entries) in &mut by_format {
        for i in (1..entries.len()).rev() {
            let j = rng.below(i + 1);
            entries.swap(i, j);
        }
        if *f == Format::Android {
            for _ in 0..(entries.len() / 150).max(1) {
                let at = rng.below(entries.len());
                entries.insert(
                    at,
                    Entry {
                        person: usize::MAX,
                        ..Entry::default()
                    },
                );
            }
        }
    }

    let names = ["icloud.vcf", "google.csv", "android.vcf", "outlook.csv"];
    let mut truth_entries = Vec::new();
    let mut next_empty = persons.len();
    for ((fmt, entries), name) in by_format.iter().zip(names) {
        let bytes: Vec<u8> = match fmt {
            Format::ICloud => icloud(entries).into_bytes(),
            Format::Google => google(entries).into_bytes(),
            Format::Android => android(&mut rng, entries).into_bytes(),
            Format::Outlook => outlook(entries),
        };
        std::fs::write(dir.join(name), bytes)?;
        for (i, e) in entries.iter().enumerate() {
            let person = if e.person == usize::MAX {
                next_empty += 1;
                next_empty - 1
            } else {
                e.person
            };
            truth_entries.push(json!({
                "file": name,
                "index": i,
                "person": person,
                "empty": e.person == usize::MAX,
                "damage": e.damage,
            }));
        }
    }
    let total = truth_entries.len();
    let truth = json!({
        "seed": seed,
        "people": persons.len(),
        "files": names,
        "entries": truth_entries,
        "traps": traps,
    });
    let mut s = String::new();
    writeln!(s, "{}", serde_json::to_string(&truth)?)?;
    std::fs::write(dir.join("truth.json"), s)?;
    println!(
        "{} people, {} entries in {}: {}",
        persons.len(),
        total,
        dir.display(),
        by_format
            .iter()
            .zip(names)
            .map(|((_, e), n)| format!("{n} {}", e.len()))
            .collect::<Vec<_>>()
            .join(", ")
    );
    Ok(())
}
