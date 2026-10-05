//! vCard 2.1, 3.0 and 4.0 reading.

use crate::import::decode_charset;

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
    let head = line.split(':').next().unwrap_or("");
    head.to_ascii_uppercase().contains(word)
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
            if line.starts_with(' ') || line.starts_with('\t') {
                cur.push_str(&line[1..]);
                continue;
            }
            if cur.ends_with('=') && header_has(cur, "QUOTED-PRINTABLE") {
                cur.pop();
                cur.push_str(line);
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
    // The value starts at the first colon that isn't inside a quoted
    // parameter value (`LABEL="a:b"`).
    let mut quoted = false;
    let mut split = None;
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            ':' if !quoted => {
                split = Some(i);
                break;
            }
            _ => {}
        }
    }
    let split = split?;
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
