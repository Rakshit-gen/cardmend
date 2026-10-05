//! Google Contacts CSV (both the current and the older "Given Name" layout)
//! and Outlook CSV.

use std::collections::HashMap;

use crate::contact::{Address, Contact, Field, Name, Photo, Source, clean_date};
use crate::import::Issue;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Google,
    Outlook,
}

/// Which CSV this is, from its header row. None if it's neither.
pub fn detect(header: &[String]) -> Option<Layout> {
    let has = |h: &str| header.iter().any(|x| x.eq_ignore_ascii_case(h));
    if has("Phone 1 - Value") || has("E-mail 1 - Value") || has("Given Name") {
        Some(Layout::Google)
    } else if has("E-mail Address") || has("Mobile Phone") || has("Business Phone") {
        Some(Layout::Outlook)
    } else {
        None
    }
}

struct Row<'a> {
    cols: &'a HashMap<String, usize>,
    rec: &'a csv::StringRecord,
}

impl Row<'_> {
    fn get(&self, name: &str) -> String {
        self.cols
            .get(&name.to_ascii_lowercase())
            .and_then(|&i| self.rec.get(i))
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    }
}

pub fn read(text: &str, file: &str) -> (Vec<Contact>, Vec<Issue>) {
    let mut issues = Vec::new();
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .has_headers(true)
        .from_reader(text.as_bytes());
    let header: Vec<String> = match rdr.headers() {
        Ok(h) => h.iter().map(|s| s.trim().to_string()).collect(),
        Err(e) => {
            issues.push(Issue {
                file: file.into(),
                line: 1,
                message: format!("couldn't read the header row: {e}"),
            });
            return (Vec::new(), issues);
        }
    };
    let Some(layout) = detect(&header) else {
        issues.push(Issue {
            file: file.into(),
            line: 1,
            message: "this CSV isn't a Google Contacts or Outlook export (no phone or email columns found)".into(),
        });
        return (Vec::new(), issues);
    };
    let cols: HashMap<String, usize> = header
        .iter()
        .enumerate()
        .map(|(i, h)| (h.to_ascii_lowercase(), i))
        .collect();
    let mut out = Vec::new();
    for rec in rdr.records() {
        let rec = match rec {
            Ok(r) => r,
            Err(e) => {
                let line = e.position().map(|p| p.line() as usize).unwrap_or(0);
                issues.push(Issue {
                    file: file.into(),
                    line,
                    message: format!("row couldn't be read, skipped: {e}"),
                });
                continue;
            }
        };
        let line = rec.position().map(|p| p.line() as usize).unwrap_or(0);
        if rec.len() != header.len() {
            issues.push(Issue {
                file: file.into(),
                line,
                message: format!(
                    "row has {} columns but the header has {}; read the ones that line up",
                    rec.len(),
                    header.len()
                ),
            });
        }
        if rec.iter().all(|f| f.trim().is_empty()) {
            continue;
        }
        let row = Row {
            cols: &cols,
            rec: &rec,
        };
        let mut c = match layout {
            Layout::Google => google(&row),
            Layout::Outlook => outlook(&row),
        };
        c.source = Source {
            file: file.into(),
            index: out.len(),
            line,
        };
        out.push(c);
    }
    (out, issues)
}

/// Google puts several values in one cell separated by " ::: ".
fn multi(cell: &str) -> Vec<String> {
    cell.split(":::")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// A Google label ("Mobile", "* Home", "Work Fax", "Grandma's") as vCard
/// types plus a custom label for anything without a standard type.
fn google_label<T>(value: T, raw: &str) -> Field<T> {
    let raw = raw.trim().trim_start_matches('*').trim();
    let mut f = Field::new(value);
    if raw.is_empty() {
        return f;
    }
    let types: &[&str] = match raw.to_ascii_lowercase().as_str() {
        "mobile" | "cell" => &["cell"],
        "home" => &["home"],
        "work" => &["work"],
        "home fax" => &["home", "fax"],
        "work fax" => &["work", "fax"],
        "pager" => &["pager"],
        "other" | "" => &[],
        _ => {
            f.label = Some(raw.to_string());
            &[]
        }
    };
    f.types = types.iter().map(|t| t.to_string()).collect();
    if raw.eq_ignore_ascii_case("other") {
        f.label = Some("Other".into());
    }
    f
}

fn google(r: &Row) -> Contact {
    let mut c = Contact::default();
    let given = first_of(r, &["First Name", "Given Name"]);
    let family = first_of(r, &["Last Name", "Family Name"]);
    let additional = first_of(r, &["Middle Name", "Additional Name"]);
    c.name = Name {
        family,
        given,
        additional,
        prefix: r.get("Name Prefix"),
        suffix: r.get("Name Suffix"),
    };
    c.formatted_name = r.get("Name");
    c.nicknames = multi(&r.get("Nickname"));
    c.org = first_of(r, &["Organization Name", "Organization 1 - Name"]);
    c.title = first_of(r, &["Organization Title", "Organization 1 - Title"]);
    c.department = first_of(
        r,
        &["Organization Department", "Organization 1 - Department"],
    );
    c.birthday = clean_date(&r.get("Birthday"));
    c.note = r.get("Notes").replace("\r\n", "\n");
    let photo = r.get("Photo");
    if !photo.is_empty() {
        c.photo = Some(Photo::Uri(photo));
    }
    c.categories = multi(&first_of(r, &["Labels", "Group Membership"]))
        .into_iter()
        .map(|s| s.trim_start_matches('*').trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    for i in 1.. {
        let key = |what: &str, part: &str| format!("{what} {i} - {part}");
        let present = ["E-mail", "Phone", "Address", "Website"].iter().any(|w| {
            r.cols.contains_key(&key(w, "Value").to_ascii_lowercase())
                || r.cols
                    .contains_key(&key(w, "Formatted").to_ascii_lowercase())
                || r.cols.contains_key(&key(w, "Street").to_ascii_lowercase())
        });
        if !present {
            break;
        }
        let label = |w: &str| {
            let l = r.get(&key(w, "Label"));
            if l.is_empty() {
                r.get(&key(w, "Type"))
            } else {
                l
            }
        };
        for v in multi(&r.get(&key("Phone", "Value"))) {
            c.phones.push(google_label(v, &label("Phone")));
        }
        for v in multi(&r.get(&key("E-mail", "Value"))) {
            c.emails.push(google_label(v, &label("E-mail")));
        }
        for v in multi(&r.get(&key("Website", "Value"))) {
            c.urls.push(google_label(v, &label("Website")));
        }
        let a = Address {
            po_box: r.get(&key("Address", "PO Box")),
            extended: r.get(&key("Address", "Extended Address")),
            street: r.get(&key("Address", "Street")),
            locality: r.get(&key("Address", "City")),
            region: r.get(&key("Address", "Region")),
            postal_code: r.get(&key("Address", "Postal Code")),
            country: r.get(&key("Address", "Country")),
        };
        let a = if a.is_empty() {
            // Some rows only fill the formatted address; keep it as the street.
            Address {
                street: r.get(&key("Address", "Formatted")).replace('\n', ", "),
                ..a
            }
        } else {
            a
        };
        if !a.is_empty() {
            c.addresses.push(google_label(a, &label("Address")));
        }
    }
    c
}

fn first_of(r: &Row, names: &[&str]) -> String {
    names
        .iter()
        .map(|n| r.get(n))
        .find(|v| !v.is_empty())
        .unwrap_or_default()
}

fn outlook(r: &Row) -> Contact {
    let mut c = Contact {
        name: Name {
            family: r.get("Last Name"),
            given: r.get("First Name"),
            additional: r.get("Middle Name"),
            // Outlook's "Title" column is the honorific; the job is "Job Title".
            prefix: r.get("Title"),
            suffix: r.get("Suffix"),
        },
        nicknames: multi(&r.get("Nickname")),
        org: r.get("Company"),
        department: r.get("Department"),
        title: r.get("Job Title"),
        birthday: clean_date(&r.get("Birthday")),
        note: r.get("Notes").replace("\r\n", "\n"),
        categories: r
            .get("Categories")
            .split(';')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        ..Contact::default()
    };
    let phones: [(&str, &[&str], Option<&str>); 13] = [
        ("Mobile Phone", &["cell"], None),
        ("Home Phone", &["home"], None),
        ("Home Phone 2", &["home"], None),
        ("Business Phone", &["work"], None),
        ("Business Phone 2", &["work"], None),
        ("Company Main Phone", &["work"], Some("Main")),
        ("Primary Phone", &["pref"], None),
        ("Other Phone", &[], Some("Other")),
        ("Car Phone", &[], Some("Car")),
        ("Pager", &["pager"], None),
        ("Business Fax", &["work", "fax"], None),
        ("Home Fax", &["home", "fax"], None),
        ("Assistant's Phone", &[], Some("Assistant")),
    ];
    for (col, types, label) in phones {
        let v = r.get(col);
        if !v.is_empty() {
            let mut f = Field::typed(v, types);
            f.label = label.map(String::from);
            c.phones.push(f);
        }
    }
    for col in ["E-mail Address", "E-mail 2 Address", "E-mail 3 Address"] {
        let v = r.get(col);
        // Exchange accounts export an X.500 path instead of an address.
        if !v.is_empty() && !v.starts_with("/o=") {
            c.emails.push(Field::new(v));
        }
    }
    for (prefix, types) in [
        ("Home", &["home"][..]),
        ("Business", &["work"]),
        ("Other", &[]),
    ] {
        let street = [
            r.get(&format!("{prefix} Street")),
            r.get(&format!("{prefix} Street 2")),
            r.get(&format!("{prefix} Street 3")),
        ]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
        let a = Address {
            po_box: r.get(&format!("{prefix} PO Box")),
            extended: String::new(),
            street,
            locality: r.get(&format!("{prefix} City")),
            region: r.get(&format!("{prefix} State")),
            postal_code: r.get(&format!("{prefix} Postal Code")),
            country: r.get(&format!("{prefix} Country/Region")),
        };
        if !a.is_empty() {
            let mut f = Field::typed(a, types);
            if prefix == "Other" {
                f.label = Some("Other".into());
            }
            c.addresses.push(f);
        }
    }
    for (col, types) in [
        ("Web Page", &[][..]),
        ("Personal Web Page", &["home"]),
        ("Business Web Page", &["work"]),
    ] {
        let v = r.get(col);
        if !v.is_empty() {
            c.urls.push(Field::typed(v, types));
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn google_current_layout() {
        let csv = "First Name,Middle Name,Last Name,Nickname,Organization Name,Organization Title,Birthday,Notes,Photo,Labels,E-mail 1 - Label,E-mail 1 - Value,Phone 1 - Label,Phone 1 - Value,Phone 2 - Label,Phone 2 - Value,Address 1 - Label,Address 1 - Formatted,Address 1 - Street,Address 1 - City,Address 1 - Postal Code,Address 1 - Country\n\
            Robert,,Smith,Bob,Acme,Manager,--03-07,\"Two\nlines\",https://lh3.example/p,* myContacts ::: Work,* Work,rob@acme.example ::: bob@gmail.com,Mobile,+1 415 555 0134 ::: +1 415 555 0135,Grandma's,020 7946 0018,Home,,12 Elm St,Springfield,94110,US\n";
        let (cs, issues) = read(csv, "g.csv");
        assert!(issues.is_empty(), "{issues:?}");
        let c = &cs[0];
        assert_eq!(c.name.given, "Robert");
        assert_eq!(c.nicknames, ["Bob"]);
        assert_eq!(c.birthday.as_deref(), Some("--03-07"));
        assert_eq!(c.note, "Two\nlines");
        assert_eq!(c.emails.len(), 2);
        assert_eq!(c.emails[0].types, ["work"]);
        assert_eq!(c.phones.len(), 3);
        assert_eq!(c.phones[1].types, ["cell"]);
        assert_eq!(c.phones[2].label.as_deref(), Some("Grandma's"));
        assert_eq!(c.addresses[0].value.locality, "Springfield");
        assert_eq!(c.addresses[0].types, ["home"]);
        assert_eq!(c.categories, ["myContacts", "Work"]);
        assert_eq!(c.source.line, 2);
    }

    #[test]
    fn google_older_layout() {
        let csv = "Name,Given Name,Family Name,Group Membership,E-mail 1 - Type,E-mail 1 - Value,Phone 1 - Type,Phone 1 - Value,Organization 1 - Name\n\
            Liz Taylor,Liz,Taylor,* myContacts,* Home,liz@example.com,Mobile,+44 7911 123456,BBC\n";
        let (cs, _) = read(csv, "g.csv");
        assert_eq!(cs[0].formatted_name, "Liz Taylor");
        assert_eq!(cs[0].emails[0].types, ["home"]);
        assert_eq!(cs[0].phones[0].types, ["cell"]);
        assert_eq!(cs[0].org, "BBC");
    }

    #[test]
    fn outlook_layout() {
        let csv = "\"First Name\",\"Middle Name\",\"Last Name\",\"Title\",\"Suffix\",\"Company\",\"Job Title\",\"Business Street\",\"Business City\",\"Business Postal Code\",\"Business Phone\",\"Mobile Phone\",\"Home Phone\",\"E-mail Address\",\"E-mail 2 Address\",\"Birthday\",\"Notes\",\"Categories\"\n\
            \"Jürgen\",\"\",\"Müller\",\"Dr.\",\"\",\"Bosch\",\"Engineer\",\"Robert-Bosch-Platz 1\",\"Stuttgart\",\"70839\",\"+49 711 400 40990\",\"0151 23456789\",\"\",\"j.mueller@bosch.example\",\"/o=ExchangeLabs/ou=Exchange\",\"3/7/1984\",\"\",\"Work;Germany\"\n\
            \"Ann\",\"\",\"Lee\",\"\",\"\",\"\",\"\",\"\",\"\",\"\",\"\",\"\",\"\",\"\",\"\",\"0/0/00\",\"\",\"\"\n";
        let (cs, issues) = read(csv, "o.csv");
        assert!(issues.is_empty(), "{issues:?}");
        let c = &cs[0];
        assert_eq!(c.name.prefix, "Dr.");
        assert_eq!(c.title, "Engineer");
        assert_eq!(c.phones[0].types, ["cell"]);
        assert_eq!(c.phones[0].value, "0151 23456789");
        assert_eq!(c.phones[1].types, ["work"]);
        assert_eq!(c.emails.len(), 1, "X.500 path is not an email");
        assert_eq!(c.birthday.as_deref(), Some("1984-03-07"));
        assert_eq!(c.addresses[0].value.locality, "Stuttgart");
        assert_eq!(c.categories, ["Work", "Germany"]);
        assert_eq!(cs[1].birthday, None);
    }

    #[test]
    fn reports_short_rows_and_unknown_csv() {
        let csv = "First Name,Last Name,Mobile Phone\nAnn,Lee\n\nBo,Ek,123\n";
        let (cs, issues) = read(csv, "o.csv");
        assert_eq!(cs.len(), 2);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].line, 2);
        let (cs, issues) = read("a,b\n1,2\n", "x.csv");
        assert!(cs.is_empty());
        assert!(issues[0].message.contains("isn't a Google"));
    }
}
