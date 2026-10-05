//! Reading export files: working out the text encoding and which of the
//! supported formats a file is, then handing it to the right parser.

use serde::Serialize;

use crate::contact::Contact;

/// Something in a file that couldn't be read as written. The rest of the
/// file is still used.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Issue {
    pub file: String,
    /// Line in the file (CSV: the row's first line), from 1. 0 when the
    /// problem is with the file as a whole.
    pub line: usize,
    pub message: String,
}

/// Bytes to text. Exports come as UTF-8 (with or without a BOM), UTF-16
/// (Outlook's "Unicode" CSV) or a Windows code page (older Outlook and
/// Android 2.1 files).
pub fn decode_text(bytes: &[u8]) -> String {
    if let Some((enc, bom_len)) = encoding_rs::Encoding::for_bom(bytes) {
        let (text, _) = enc.decode_without_bom_handling(&bytes[bom_len..]);
        return text.into_owned();
    }
    // UTF-16 without a BOM still has a zero byte in every ASCII character.
    if bytes.len() >= 4 && bytes[1] == 0 && bytes[3] == 0 {
        return encoding_rs::UTF_16LE
            .decode_without_bom_handling(bytes)
            .0
            .into_owned();
    }
    if bytes.len() >= 4 && bytes[0] == 0 && bytes[2] == 0 {
        return encoding_rs::UTF_16BE
            .decode_without_bom_handling(bytes)
            .0
            .into_owned();
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => encoding_rs::WINDOWS_1252.decode(bytes).0.into_owned(),
    }
}

/// Decode bytes in a named charset (a vCard CHARSET parameter). Unknown
/// names and invalid UTF-8 fall back to Windows-1252, which never fails and
/// is what most non-UTF-8 phone exports actually used.
pub fn decode_charset(bytes: &[u8], charset: Option<&str>) -> String {
    let enc = charset
        .and_then(|c| encoding_rs::Encoding::for_label(c.trim().as_bytes()))
        .unwrap_or(encoding_rs::UTF_8);
    if enc == encoding_rs::UTF_8 {
        if let Ok(s) = std::str::from_utf8(bytes) {
            return s.to_string();
        }
        return encoding_rs::WINDOWS_1252.decode(bytes).0.into_owned();
    }
    enc.decode(bytes).0.into_owned()
}

/// Read one export file of any supported kind. The format is decided by
/// content, not the file name, since people rename exports.
pub fn read_file(name: &str, bytes: &[u8]) -> (Vec<Contact>, Vec<Issue>) {
    let text = decode_text(bytes);
    let head = text.trim_start();
    if head.len() >= 11 && head[..11].eq_ignore_ascii_case("BEGIN:VCARD") {
        return crate::vcard::read(&text, name);
    }
    let first_line = head.lines().next().unwrap_or("");
    if first_line.contains(',') {
        return crate::csv_import::read(&text, name);
    }
    let message = if text.trim().is_empty() {
        "the file is empty".to_string()
    } else {
        "not a contacts export: expected a .vcf starting with BEGIN:VCARD, or a Google or Outlook CSV".to_string()
    };
    (
        Vec::new(),
        vec![Issue {
            file: name.to_string(),
            line: 0,
            message,
        }],
    )
}

/// Every contact from every file, numbered in order.
#[derive(Debug, Default, Clone, Serialize)]
pub struct Book {
    pub contacts: Vec<Contact>,
    pub issues: Vec<Issue>,
    /// File names with how many contacts each gave.
    pub files: Vec<(String, usize)>,
}

impl Book {
    pub fn add(&mut self, name: &str, bytes: &[u8]) {
        let (cs, mut issues) = read_file(name, bytes);
        // The reader notices a missing END:VCARD only at the next card, so
        // put problems back in file order for reading top to bottom.
        issues.sort_by_key(|i| i.line);
        self.files.push((name.to_string(), cs.len()));
        for mut c in cs {
            c.id = self.contacts.len();
            self.contacts.push(c);
        }
        self.issues.extend(issues);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_format_by_content() {
        let mut b = Book::default();
        b.add(
            "a.txt",
            b"\xEF\xBB\xBF\r\nBEGIN:VCARD\r\nFN:A\r\nEND:VCARD\r\n",
        );
        b.add("b.vcf", b"First Name,Last Name,Mobile Phone\nBo,Ek,1\n");
        b.add("c.vcf", b"hello");
        b.add("d.vcf", b"");
        assert_eq!(b.contacts.len(), 2);
        assert_eq!(b.contacts[1].id, 1);
        assert_eq!(b.contacts[1].name.given, "Bo");
        assert_eq!(b.issues.len(), 2);
        assert!(b.issues[1].message.contains("empty"));
    }

    #[test]
    fn strips_utf8_bom() {
        assert_eq!(decode_text(b"\xEF\xBB\xBFBEGIN:VCARD"), "BEGIN:VCARD");
    }

    #[test]
    fn reads_utf16_with_and_without_bom() {
        let le: Vec<u8> = "Name,Phone"
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .collect();
        let mut with_bom = vec![0xFF, 0xFE];
        with_bom.extend(&le);
        assert_eq!(decode_text(&with_bom), "Name,Phone");
        assert_eq!(decode_text(&le), "Name,Phone");
    }

    #[test]
    fn falls_back_to_windows_1252() {
        // "José" saved by an old Outlook: é is 0xE9.
        assert_eq!(decode_text(b"Jos\xE9"), "José");
    }

    #[test]
    fn decodes_named_charsets() {
        assert_eq!(decode_charset(b"M\xFCller", Some("ISO-8859-1")), "Müller");
        assert_eq!(decode_charset("Müller".as_bytes(), Some("UTF-8")), "Müller");
        assert_eq!(decode_charset(b"M\xFCller", None), "Müller");
    }
}
