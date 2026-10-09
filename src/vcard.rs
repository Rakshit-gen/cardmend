//! vCard 2.1, 3.0 and 4.0 reading.

use base64::Engine;

use crate::contact::{Address, Contact, Extra, Field, Name, Photo, Source};
use crate::import::{Issue, decode_charset};

/// One property after unfolding, with its parameters split out and its
/// value decoded from quoted-printable (but not yet unescaped).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prop {
    pub line: usize,
    pub group: Option<String>,
    /// Uppercased: `TEL`, `X-ABLABEL`.
    pub name: String,
    /// Names uppercased, values as written. 2.1's bare parameters (`TEL;CELL`)
    /// become `TYPE` entries, so callers only look in one place.
    pub params: Vec<(String, String)>,
    pub value: String,
}

impl Prop {
    pub fn param(&self, name: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// TYPE values, lowercased, with comma lists split: `TYPE=CELL,VOICE`
    /// and `type=cell;type=voice` both give `["cell", "voice"]`.
    pub fn types(&self) -> Vec<String> {
        self.params
            .iter()
            .filter(|(k, _)| k == "TYPE")
            .flat_map(|(_, v)| v.split(','))
            .map(|t| t.trim().to_ascii_lowercase())
            .filter(|t| !t.is_empty())
            .collect()
    }

    fn is_base64(&self) -> bool {
        self.param("ENCODING")
            .is_some_and(|e| e.eq_ignore_ascii_case("b") || e.eq_ignore_ascii_case("base64"))
    }
}

/// Header of a raw line (everything before the value) mentions QP. Checked
/// before parsing because QP soft line breaks change how lines join.
fn header_has(line: &str, word: &str) -> bool {
    line[..value_start(line).unwrap_or(line.len())]
        .to_ascii_uppercase()
        .contains(word)
}

/// The colon that starts the value: the first one that isn't inside a
/// quoted parameter value (`LABEL="a:b"`).
fn value_start(line: &str) -> Option<usize> {
    let mut quoted = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            ':' if !quoted => return Some(i),
            _ => {}
        }
    }
    None
}

fn looks_like_base64(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'=' | b' ' | b'\t'))
}

/// Join folded lines back into logical lines, each with the line number it
/// started on. Handles three kinds of continuation:
/// - RFC folding: a line starting with a space or tab continues the last one;
/// - quoted-printable soft breaks: a QP value ending in `=` continues on the
///   next line, which is not indented;
/// - base64 blocks in 2.1 files where the continuation lines aren't indented.
pub fn unfold(text: &str) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String)> = Vec::new();
    let mut open = false;
    for (i, raw) in text.split('\n').enumerate() {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.is_empty() {
            // A blank line ends a 2.1 base64 block; otherwise it means nothing.
            open = false;
            continue;
        }
        if let Some(last) = out.last_mut().filter(|_| open) {
            let cur = &mut last.1;
            // QP first: after a soft break, a leading space is part of the text.
            if cur.ends_with('=') && header_has(cur, "QUOTED-PRINTABLE") {
                cur.pop();
                cur.push_str(line);
                continue;
            }
            if line.starts_with(' ') || line.starts_with('\t') {
                cur.push_str(&line[1..]);
                continue;
            }
            if !line.contains(':') && header_has(cur, "BASE64") && looks_like_base64(line) {
                cur.push_str(line);
                continue;
            }
        }
        out.push((i + 1, line.to_string()));
        open = true;
    }
    out
}

/// Split a logical line into a property. Returns None for lines with no
/// colon, which are not properties.
pub fn parse_line(line_no: usize, line: &str) -> Option<Prop> {
    let split = value_start(line)?;
    let (head, value) = (&line[..split], &line[split + 1..]);

    let mut parts = split_params(head).into_iter();
    let full_name = parts.next()?;
    let (group, name) = match full_name.rsplit_once('.') {
        Some((g, n)) => (Some(g.to_string()), n),
        None => (None, full_name.as_str()),
    };
    let name = name.trim().to_ascii_uppercase();
    if name.is_empty() {
        return None;
    }
    let mut params = Vec::new();
    for p in parts {
        let (k, v) = match p.split_once('=') {
            Some((k, v)) => (
                k.trim().to_ascii_uppercase(),
                v.trim().trim_matches('"').to_string(),
            ),
            None => {
                let bare = p.trim().to_string();
                let up = bare.to_ascii_uppercase();
                // 2.1 allows `;QUOTED-PRINTABLE` and `;BASE64` without a name.
                if matches!(up.as_str(), "QUOTED-PRINTABLE" | "BASE64" | "8BIT" | "7BIT") {
                    ("ENCODING".to_string(), bare)
                } else {
                    ("TYPE".to_string(), bare)
                }
            }
        };
        if !k.is_empty() {
            params.push((k, v));
        }
    }
    let mut prop = Prop {
        line: line_no,
        group,
        name,
        params,
        value: value.to_string(),
    };
    if prop
        .param("ENCODING")
        .is_some_and(|e| e.eq_ignore_ascii_case("quoted-printable"))
    {
        let bytes = decode_qp(&prop.value);
        prop.value = decode_charset(&bytes, prop.param("CHARSET"));
        prop.params
            .retain(|(k, _)| k != "ENCODING" && k != "CHARSET");
    } else if prop.param("CHARSET").is_some() && !prop.is_base64() {
        // The file was already decoded as a whole; the parameter has done its job.
        prop.params.retain(|(k, _)| k != "CHARSET");
    }
    Some(prop)
}

fn split_params(head: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut quoted = false;
    for c in head.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                out.last_mut().unwrap().push(c);
            }
            ';' if !quoted => out.push(String::new()),
            _ => out.last_mut().unwrap().push(c),
        }
    }
    out
}

/// Quoted-printable to bytes. Malformed escapes are kept as written rather
/// than dropped, so nothing silently disappears.
pub fn decode_qp(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'='
            && i + 2 < b.len()
            && let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2]))
        {
            out.push(h << 4 | l);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

fn hex(c: u8) -> Option<u8> {
    (c as char).to_digit(16).map(|d| d as u8)
}

/// Split on a separator that isn't escaped with a backslash, then unescape
/// each piece. Used with `;` for N and ADR and `,` for lists.
pub fn split_unescape(value: &str, sep: char) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n' | 'N') => out.last_mut().unwrap().push('\n'),
                Some(other) => out.last_mut().unwrap().push(other),
                None => out.last_mut().unwrap().push('\\'),
            }
        } else if c == sep {
            out.push(String::new());
        } else {
            out.last_mut().unwrap().push(c);
        }
    }
    out
}

/// Undo text escaping: `\n`, `\,`, `\;`, `\\`.
pub fn unescape(value: &str) -> String {
    // No separator char can appear unescaped in a way we'd split on here.
    split_unescape(value, '\u{0}').concat()
}

/// Read every contact in a vCard file. Problems are reported with their
/// line number and the rest of the file is still read.
pub fn read(text: &str, file: &str) -> (Vec<Contact>, Vec<Issue>) {
    let mut contacts = Vec::new();
    let mut issues = Vec::new();
    let issue = |line: usize, message: String| Issue {
        file: file.to_string(),
        line,
        message,
    };
    let mut current: Option<(Contact, Vec<Prop>)> = None;
    for (line, text) in unfold(text) {
        let Some(prop) = parse_line(line, &text) else {
            if current.is_some() {
                issues.push(issue(
                    line,
                    format!("not a vCard property, skipped: {}", clip(&text)),
                ));
            }
            continue;
        };
        match (prop.name.as_str(), current.is_some()) {
            ("BEGIN", false) if prop.value.eq_ignore_ascii_case("VCARD") => {
                let c = Contact {
                    source: Source {
                        file: file.to_string(),
                        index: contacts.len(),
                        line,
                    },
                    ..Contact::default()
                };
                current = Some((c, Vec::new()));
            }
            ("BEGIN", true) if prop.value.eq_ignore_ascii_case("VCARD") => {
                let (c, props) = current.take().unwrap();
                issues.push(issue(
                    c.source.line,
                    "contact has no END:VCARD before the next one starts; read what was there"
                        .into(),
                ));
                contacts.push(build(c, props, &mut issues, file));
                let c = Contact {
                    source: Source {
                        file: file.to_string(),
                        index: contacts.len(),
                        line,
                    },
                    ..Contact::default()
                };
                current = Some((c, Vec::new()));
            }
            ("END", true) if prop.value.eq_ignore_ascii_case("VCARD") => {
                let (c, props) = current.take().unwrap();
                contacts.push(build(c, props, &mut issues, file));
            }
            (_, true) => current.as_mut().unwrap().1.push(prop),
            (_, false) => issues.push(issue(
                line,
                format!(
                    "{} is outside any BEGIN:VCARD ... END:VCARD, skipped",
                    prop.name
                ),
            )),
        }
    }
    if let Some((c, props)) = current {
        issues.push(issue(
            c.source.line,
            "file ends before this contact's END:VCARD; read what was there".into(),
        ));
        contacts.push(build(c, props, &mut issues, file));
    }
    (contacts, issues)
}

fn clip(s: &str) -> String {
    match s.char_indices().nth(40) {
        Some((i, _)) => format!("{}...", &s[..i]),
        None => s.to_string(),
    }
}

fn field(p: &Prop, value: String) -> Field<String> {
    Field {
        value,
        types: p.types(),
        label: None,
    }
}

fn build(mut c: Contact, props: Vec<Prop>, issues: &mut Vec<Issue>, file: &str) -> Contact {
    // Groups (`item1.TEL` + `item1.X-ABLabel`) tie a custom label to a
    // value. Labels are applied once every field has been read.
    let mut labels: Vec<(String, String)> = Vec::new();
    let mut grouped: Vec<(String, &'static str, usize)> = Vec::new();
    let mut note_parts: Vec<String> = Vec::new();
    for p in props {
        let mut slot = |kind: &'static str, len: usize| {
            if let Some(g) = &p.group {
                grouped.push((g.clone(), kind, len - 1));
            }
        };
        match p.name.as_str() {
            "VERSION" | "PRODID" | "REV" => {}
            "FN" => c.formatted_name = unescape(&p.value).trim().to_string(),
            "N" => {
                let v = split_unescape(&p.value, ';');
                let g = |i: usize| v.get(i).map(|s| s.trim().to_string()).unwrap_or_default();
                c.name = Name {
                    family: g(0),
                    given: g(1),
                    additional: g(2),
                    prefix: g(3),
                    suffix: g(4),
                };
            }
            "NICKNAME" => c.nicknames.extend(
                split_unescape(&p.value, ',')
                    .into_iter()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
            ),
            "ORG" => {
                let v = split_unescape(&p.value, ';');
                c.org = v.first().map(|s| s.trim().to_string()).unwrap_or_default();
                c.department = v[1..]
                    .iter()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(", ");
            }
            "TITLE" => c.title = unescape(&p.value).trim().to_string(),
            "TEL" => {
                let v = unescape(&p.value);
                // 4.0 writes numbers as `tel:+1-...` URIs.
                let v = v.strip_prefix("tel:").unwrap_or(&v).trim().to_string();
                if !v.is_empty() {
                    c.phones.push(field(&p, v));
                    slot("TEL", c.phones.len());
                }
            }
            "EMAIL" => {
                let v = unescape(&p.value).trim().to_string();
                if !v.is_empty() {
                    let mut f = field(&p, v);
                    // Every Apple and Google export says INTERNET; it tells us nothing.
                    f.types.retain(|t| t != "internet" && t != "x400");
                    c.emails.push(f);
                    slot("EMAIL", c.emails.len());
                }
            }
            "ADR" => {
                let a = Address::from_parts(
                    &split_unescape(&p.value, ';')
                        .into_iter()
                        .map(|s| s.trim().to_string())
                        .collect::<Vec<_>>(),
                );
                if !a.is_empty() {
                    c.addresses.push(Field {
                        value: a,
                        types: p.types(),
                        label: None,
                    });
                    slot("ADR", c.addresses.len());
                }
            }
            "URL" => {
                let v = unescape(&p.value).trim().to_string();
                if !v.is_empty() {
                    c.urls.push(field(&p, v));
                    slot("URL", c.urls.len());
                }
            }
            "BDAY" => {
                let v = p.value.trim().to_string();
                // Apple marks a birthday with no year by putting 1604 in it.
                let v = match (p.param("X-APPLE-OMIT-YEAR"), v.get(4..)) {
                    (Some(y), Some(rest)) if v.starts_with(y) => format!("-{rest}"),
                    _ => v,
                };
                c.birthday = crate::contact::clean_date(&v);
            }
            "NOTE" => {
                let v = unescape(&p.value).replace("\r\n", "\n").trim().to_string();
                if !v.is_empty() {
                    note_parts.push(v);
                }
            }
            "PHOTO" => match read_photo(&p) {
                Ok(Some(photo)) => {
                    if c.photo.as_ref().is_none_or(|old| photo.size() > old.size()) {
                        c.photo = Some(photo);
                    }
                }
                Ok(None) => {}
                Err(e) => issues.push(Issue {
                    file: file.to_string(),
                    line: p.line,
                    message: format!(
                        "photo couldn't be read ({e}); the contact is kept without it"
                    ),
                }),
            },
            "CATEGORIES" => c.categories.extend(
                split_unescape(&p.value, ',')
                    .into_iter()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
            ),
            "UID" => c.uid = Some(p.value.trim().to_string()),
            "X-ABLABEL" if p.group.is_some() => {
                labels.push((p.group.clone().unwrap(), unescape(&p.value)))
            }
            _ => c.extra.push(Extra {
                group: p.group.clone(),
                name: p.name.clone(),
                params: p.params.clone(),
                value: p.value.clone(),
            }),
        }
    }
    c.note = note_parts.join("\n\n");
    for (group, label) in labels {
        let label = clean_label(&label);
        let target = grouped.iter().find(|(g, _, _)| *g == group);
        match target {
            Some((_, "TEL", i)) => c.phones[*i].label = Some(label),
            Some((_, "EMAIL", i)) => c.emails[*i].label = Some(label),
            Some((_, "ADR", i)) => c.addresses[*i].label = Some(label),
            Some((_, "URL", i)) => c.urls[*i].label = Some(label),
            // A label for something we keep as-is (X-ABDATE, X-ABRELATEDNAMES).
            _ => c.extra.push(Extra {
                group: Some(group),
                name: "X-ABLABEL".into(),
                params: Vec::new(),
                value: label,
            }),
        }
    }
    c
}

/// Apple wraps its built-in labels as `_$!<Mobile>!$_`.
fn clean_label(raw: &str) -> String {
    let t = raw.trim();
    t.strip_prefix("_$!<")
        .and_then(|s| s.strip_suffix(">!$_"))
        .unwrap_or(t)
        .to_string()
}

fn read_photo(p: &Prop) -> Result<Option<Photo>, String> {
    let value = p.value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    // 4.0: PHOTO:data:image/jpeg;base64,...
    if let Some(rest) = value.strip_prefix("data:") {
        let (meta, data) = rest.split_once(',').ok_or("data URL without a comma")?;
        let mime = meta.split(';').next().unwrap_or("").to_string();
        if !meta.contains("base64") {
            return Err("data URL isn't base64".into());
        }
        let bytes = decode_b64(data)?;
        return Ok(Some(Photo::Data {
            mime: if mime.is_empty() { sniff(&bytes) } else { mime },
            bytes,
        }));
    }
    if p.is_base64() {
        let bytes = decode_b64(value)?;
        let mime = p
            .types()
            .iter()
            .chain(p.param("MEDIATYPE").map(|m| m.to_ascii_lowercase()).iter())
            .find_map(|t| match t.as_str() {
                "jpeg" | "jpg" | "image/jpeg" => Some("image/jpeg".to_string()),
                "png" | "image/png" => Some("image/png".to_string()),
                "gif" | "image/gif" => Some("image/gif".to_string()),
                _ => None,
            })
            .unwrap_or_else(|| sniff(&bytes));
        return Ok(Some(Photo::Data { mime, bytes }));
    }
    Ok(Some(Photo::Uri(value.to_string())))
}

fn decode_b64(data: &str) -> Result<Vec<u8>, String> {
    let clean: String = data.chars().filter(|c| !c.is_whitespace()).collect();
    // Some exporters drop the trailing padding.
    let e = &base64::engine::general_purpose::STANDARD;
    e.decode(&clean)
        .or_else(|_| {
            base64::engine::general_purpose::STANDARD_NO_PAD.decode(clean.trim_end_matches('='))
        })
        .map_err(|_| "the image data isn't valid base64".to_string())
}

fn sniff(bytes: &[u8]) -> String {
    match bytes {
        [0xFF, 0xD8, ..] => "image/jpeg",
        [0x89, b'P', b'N', b'G', ..] => "image/png",
        [b'G', b'I', b'F', ..] => "image/gif",
        _ => "image/jpeg",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unfolds_rfc_continuations() {
        let t = "BEGIN:VCARD\r\nNOTE:one\r\n  two\r\n\t three\r\nEND:VCARD\r\n";
        let lines = unfold(t);
        assert_eq!(lines[1], (2, "NOTE:one two three".to_string()));
        assert_eq!(lines[2], (5, "END:VCARD".to_string()));
    }

    #[test]
    fn joins_quoted_printable_soft_breaks() {
        let t =
            "N;CHARSET=UTF-8;ENCODING=QUOTED-PRINTABLE:M=C3=BC=\nller;J=C3=BCrgen;;;\nTEL;CELL:1\n";
        let lines = unfold(t);
        assert_eq!(lines.len(), 2);
        let p = parse_line(lines[0].0, &lines[0].1).unwrap();
        assert_eq!(p.value, "Müller;Jürgen;;;");
        assert!(p.param("ENCODING").is_none());
    }

    #[test]
    fn soft_break_after_a_quoted_colon_in_the_params() {
        let t = "ADR;LABEL=\"Flat 2: rear\";ENCODING=QUOTED-PRINTABLE:;;1 Main=\n St;;;;\nTEL:1\n";
        let lines = unfold(t);
        assert_eq!(lines.len(), 2);
        let p = parse_line(lines[0].0, &lines[0].1).unwrap();
        assert_eq!(p.value, ";;1 Main St;;;;");
    }

    #[test]
    fn soft_break_only_applies_to_qp_values() {
        // A plain value can legitimately end with "=".
        let lines = unfold("NOTE:a=\nTEL:1\n");
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn joins_unindented_base64_in_21() {
        let t = "PHOTO;ENCODING=BASE64;JPEG:/9j/4AAQ\nSkZJRgABAQ\n\nTEL:1\n";
        let lines = unfold(t);
        assert_eq!(lines[0].1, "PHOTO;ENCODING=BASE64;JPEG:/9j/4AAQSkZJRgABAQ");
        assert_eq!(lines[1], (4, "TEL:1".to_string()));
    }

    #[test]
    fn parses_groups_and_params() {
        let p = parse_line(
            3,
            "item1.TEL;type=CELL;type=VOICE;type=pref:+91 98765 43210",
        )
        .unwrap();
        assert_eq!(p.group.as_deref(), Some("item1"));
        assert_eq!(p.name, "TEL");
        assert_eq!(p.types(), ["cell", "voice", "pref"]);
        assert_eq!(p.value, "+91 98765 43210");
    }

    #[test]
    fn bare_21_params_become_types() {
        let p = parse_line(1, "TEL;HOME;VOICE:022 2345 6789").unwrap();
        assert_eq!(p.types(), ["home", "voice"]);
        let p = parse_line(1, "TEL;TYPE=WORK,FAX:1").unwrap();
        assert_eq!(p.types(), ["work", "fax"]);
    }

    #[test]
    fn colon_inside_quoted_param_is_not_the_value() {
        let p = parse_line(1, "ADR;LABEL=\"Flat 2: rear\";TYPE=home:;;1 Main St;;;;").unwrap();
        assert_eq!(p.param("LABEL"), Some("Flat 2: rear"));
        assert_eq!(p.value, ";;1 Main St;;;;");
    }

    #[test]
    fn qp_with_latin1_charset() {
        let p = parse_line(1, "FN;CHARSET=ISO-8859-1;ENCODING=QUOTED-PRINTABLE:Jos=E9").unwrap();
        assert_eq!(p.value, "José");
        let p = parse_line(1, "NOTE;QUOTED-PRINTABLE:a=0D=0Ab").unwrap();
        assert_eq!(p.value, "a\r\nb");
    }

    #[test]
    fn malformed_qp_is_kept() {
        assert_eq!(decode_qp("100=%"), b"100=%");
        assert_eq!(decode_qp("a=4"), b"a=4");
        assert_eq!(decode_qp("=41"), b"A");
    }

    #[test]
    fn unescapes_and_splits() {
        assert_eq!(
            split_unescape(r"Smith\; Jones;Ann;;;", ';'),
            ["Smith; Jones", "Ann", "", "", ""]
        );
        assert_eq!(
            unescape(r"line one\nline two\, and \\ more"),
            "line one\nline two, and \\ more"
        );
        assert_eq!(
            split_unescape(r"Friends,Work\,Old", ','),
            ["Friends", "Work,Old"]
        );
    }

    #[test]
    fn line_without_colon_is_not_a_property() {
        assert!(parse_line(1, "garbage here").is_none());
    }
}
