//! Writing vCard 3.0, the version both iPhone (Contacts, iCloud) and Google
//! Contacts import without complaint.
//!
//! Rules followed, and why:
//! - CRLF line ends and folding at 75 octets (RFC 2426); folds never split a
//!   UTF-8 character, which iOS shows as garbage.
//! - N is always present, even empty: iOS rejects 3.0 cards without it.
//! - Custom labels use Apple's `itemN.X-ABLabel`, which Google reads too.
//! - Photos are inline `ENCODING=b;TYPE=JPEG`; iOS ignores data: URLs in 3.0.
//! - Birthdays without a year use Apple's `X-APPLE-OMIT-YEAR=1604` form.
//! - Contacts with only a company get `X-ABShowAs:COMPANY` so iOS lists
//!   them under the company name instead of as "No name".

use base64::Engine;

use crate::contact::{Contact, Field, Photo};

pub fn write_all(contacts: &[Contact]) -> String {
    contacts.iter().map(write).collect()
}

pub fn write(c: &Contact) -> String {
    let mut w = Writer::default();
    w.line("BEGIN:VCARD");
    w.line("VERSION:3.0");
    w.line("PRODID:-//cardmend//EN");
    let n = &c.name;
    w.prop(
        "N",
        &[&n.family, &n.given, &n.additional, &n.prefix, &n.suffix]
            .map(|s| esc(s))
            .join(";"),
    );
    w.prop("FN", &esc(&c.display_name()));
    if c.formatted_name.trim().is_empty() && n.is_empty() && !c.org.trim().is_empty() {
        w.prop("X-ABShowAs", "COMPANY");
    }
    if !c.nicknames.is_empty() {
        w.prop("NICKNAME", &list(&c.nicknames));
    }
    if !c.org.is_empty() || !c.department.is_empty() {
        let mut v = esc(&c.org);
        if !c.department.is_empty() {
            v = format!("{v};{}", esc(&c.department));
        }
        w.prop("ORG", &v);
    }
    if !c.title.is_empty() {
        w.prop("TITLE", &esc(&c.title));
    }

    // Groups the extras already use, so generated item numbers avoid them.
    let taken: Vec<&str> = c.extra.iter().filter_map(|e| e.group.as_deref()).collect();
    let mut next_item = 0;
    let mut item = || loop {
        next_item += 1;
        let g = format!("item{next_item}");
        if !taken.contains(&g.as_str()) {
            return g;
        }
    };

    for p in &c.phones {
        w.field("TEL", p, &[], &esc(&p.value), &mut item);
    }
    for e in &c.emails {
        w.field("EMAIL", e, &["INTERNET"], &esc(&e.value), &mut item);
    }
    for a in &c.addresses {
        let v = a.value.parts().map(esc).join(";");
        w.field("ADR", a, &[], &v, &mut item);
    }
    for u in &c.urls {
        w.field("URL", u, &[], &esc(&u.value), &mut item);
    }
    if let Some(b) = &c.birthday {
        match b.strip_prefix("--") {
            Some(md) if md.len() == 5 => w.line(&format!("BDAY;X-APPLE-OMIT-YEAR=1604:1604-{md}")),
            _ => w.prop("BDAY", b),
        }
    }
    if !c.note.is_empty() {
        w.prop("NOTE", &esc(&c.note));
    }
    match &c.photo {
        Some(Photo::Data { mime, bytes }) => {
            let kind = mime
                .rsplit('/')
                .next()
                .unwrap_or("jpeg")
                .to_ascii_uppercase();
            let data = base64::engine::general_purpose::STANDARD.encode(bytes);
            w.line(&format!("PHOTO;ENCODING=b;TYPE={kind}:{data}"));
        }
        Some(Photo::Uri(u)) => w.line(&format!("PHOTO;VALUE=uri:{u}")),
        None => {}
    }
    if !c.categories.is_empty() {
        w.prop("CATEGORIES", &list(&c.categories));
    }
    if let Some(uid) = &c.uid {
        w.prop("UID", uid);
    }
    for e in &c.extra {
        let mut head = match &e.group {
            Some(g) => format!("{g}.{}", e.name),
            None => e.name.clone(),
        };
        for (k, v) in &e.params {
            head.push_str(&format!(";{k}={}", param_value(v)));
        }
        w.line(&format!("{head}:{}", e.value));
    }
    w.line("END:VCARD");
    w.out
}

#[derive(Default)]
struct Writer {
    out: String,
}

impl Writer {
    fn prop(&mut self, name: &str, value: &str) {
        self.line(&format!("{name}:{value}"));
    }

    fn field<T>(
        &mut self,
        name: &str,
        f: &Field<T>,
        always: &[&str],
        value: &str,
        item: &mut impl FnMut() -> String,
    ) {
        let mut head = name.to_string();
        for t in always
            .iter()
            .map(|t| t.to_string())
            .chain(f.types.iter().map(|t| t.to_ascii_uppercase()))
        {
            head.push_str(&format!(";TYPE={}", param_value(&t)));
        }
        match &f.label {
            Some(label) => {
                let g = item();
                self.line(&format!("{g}.{head}:{value}"));
                self.line(&format!("{g}.X-ABLabel:{}", esc(label)));
            }
            None => self.line(&format!("{head}:{value}")),
        }
    }

    /// Append one logical line, folded at 75 octets.
    fn line(&mut self, line: &str) {
        let mut width = 0;
        for ch in line.chars() {
            let len = ch.len_utf8();
            if width + len > 75 {
                self.out.push_str("\r\n ");
                width = 1;
            }
            self.out.push(ch);
            width += len;
        }
        self.out.push_str("\r\n");
    }
}

/// Escape a text value for 3.0: backslash, comma, semicolon, newline.
pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.push_str("\\\\"),
            ',' => out.push_str("\\,"),
            ';' => out.push_str("\\;"),
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' | '\r' => out.push_str("\\n"),
            _ => out.push(c),
        }
    }
    out
}

fn list(items: &[String]) -> String {
    items.iter().map(|s| esc(s)).collect::<Vec<_>>().join(",")
}

fn param_value(v: &str) -> String {
    if v.contains([':', ';', ',']) {
        format!("\"{}\"", v.replace('"', "'"))
    } else {
        v.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contact::{Field, Name};

    #[test]
    fn folds_at_75_octets_without_splitting_characters() {
        let c = Contact {
            formatted_name: "Zoë".repeat(40),
            ..Contact::default()
        };
        let out = write(&c);
        for line in out.split("\r\n") {
            assert!(line.len() <= 75, "{line:?} is {} bytes", line.len());
        }
        let unfolded = out.replace("\r\n ", "");
        assert!(unfolded.contains(&format!("FN:{}", "Zoë".repeat(40))));
    }

    #[test]
    fn escapes_text() {
        assert_eq!(esc("a,b;c\\d\r\ne\nf"), "a\\,b\\;c\\\\d\\ne\\nf");
    }

    #[test]
    fn company_only_contact_shows_as_company() {
        let c = Contact {
            org: "Acme".into(),
            ..Contact::default()
        };
        let out = write(&c);
        assert!(out.contains("N:;;;;\r\n"));
        assert!(out.contains("FN:Acme\r\n"));
        assert!(out.contains("X-ABShowAs:COMPANY\r\n"));
    }

    #[test]
    fn labels_use_item_groups_that_dont_clash_with_kept_ones() {
        let mut c = Contact {
            name: Name {
                given: "Ann".into(),
                ..Name::default()
            },
            ..Contact::default()
        };
        let mut p = Field::typed("+44 20 7946 0018".to_string(), &["home"]);
        p.label = Some("Grandma's".into());
        c.phones.push(p);
        c.extra.push(crate::contact::Extra {
            group: Some("item1".into()),
            name: "X-ABDATE".into(),
            params: vec![],
            value: "2015-06-20".into(),
        });
        let out = write(&c);
        assert!(
            out.contains("item2.TEL;TYPE=HOME:+44 20 7946 0018\r\n"),
            "{out}"
        );
        assert!(out.contains("item2.X-ABLabel:Grandma's\r\n"));
        assert!(out.contains("item1.X-ABDATE:2015-06-20\r\n"));
        assert!(out.contains("FN:Ann\r\n"));
    }

    #[test]
    fn birthday_without_year_uses_apple_form() {
        let c = Contact {
            birthday: Some("--03-07".into()),
            ..Contact::default()
        };
        assert!(write(&c).contains("BDAY;X-APPLE-OMIT-YEAR=1604:1604-03-07\r\n"));
    }
}
