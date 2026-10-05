//! The contact model every importer fills and the writer reads.
//!
//! Values are kept as they appeared in the source file. Matching works on
//! normalised copies (see `normalize`), so a merged file still shows the
//! number the way the person typed it.

use base64::Engine;
use serde::{Serialize, Serializer};

/// A value with the labels it carried: vCard TYPE parameters (lowercased,
/// like `cell`, `work`, `pref`) and an optional custom label such as an
/// Apple `X-ABLabel` ("Office", "Grandma's").
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Field<T> {
    pub value: T,
    pub types: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl<T> Field<T> {
    pub fn new(value: T) -> Self {
        Field {
            value,
            types: Vec::new(),
            label: None,
        }
    }

    pub fn typed(value: T, types: &[&str]) -> Self {
        Field {
            value,
            types: types.iter().map(|t| t.to_string()).collect(),
            label: None,
        }
    }

    pub fn has_type(&self, t: &str) -> bool {
        self.types.iter().any(|x| x == t)
    }
}

/// The N property: family; given; additional; prefix; suffix.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Name {
    pub family: String,
    pub given: String,
    pub additional: String,
    pub prefix: String,
    pub suffix: String,
}

impl Name {
    pub fn is_empty(&self) -> bool {
        [
            &self.family,
            &self.given,
            &self.additional,
            &self.prefix,
            &self.suffix,
        ]
        .iter()
        .all(|s| s.trim().is_empty())
    }

    /// The parts in reading order, for when a contact has N but no FN.
    pub fn display(&self) -> String {
        [
            &self.prefix,
            &self.given,
            &self.additional,
            &self.family,
            &self.suffix,
        ]
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
    }
}

/// The ADR property's seven parts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Address {
    pub po_box: String,
    pub extended: String,
    pub street: String,
    pub locality: String,
    pub region: String,
    pub postal_code: String,
    pub country: String,
}

impl Address {
    pub fn parts(&self) -> [&str; 7] {
        [
            &self.po_box,
            &self.extended,
            &self.street,
            &self.locality,
            &self.region,
            &self.postal_code,
            &self.country,
        ]
    }

    pub fn from_parts(p: &[String]) -> Self {
        let g = |i: usize| p.get(i).cloned().unwrap_or_default();
        Address {
            po_box: g(0),
            extended: g(1),
            street: g(2),
            locality: g(3),
            region: g(4),
            postal_code: g(5),
            country: g(6),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.parts().iter().all(|s| s.trim().is_empty())
    }

    /// One line, for showing on a card.
    pub fn display(&self) -> String {
        self.parts()
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Photo {
    /// Decoded image bytes and their media type, e.g. `image/jpeg`.
    Data { mime: String, bytes: Vec<u8> },
    /// A link to the photo. Google exports these instead of the image.
    Uri(String),
}

impl Photo {
    pub fn size(&self) -> usize {
        match self {
            Photo::Data { bytes, .. } => bytes.len(),
            Photo::Uri(_) => 0,
        }
    }

    pub fn as_url(&self) -> String {
        match self {
            Photo::Data { mime, bytes } => format!(
                "data:{mime};base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            ),
            Photo::Uri(u) => u.clone(),
        }
    }
}

// The page shows photos with an <img>, so a data URL is the most useful form.
impl Serialize for Photo {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.as_url())
    }
}

/// A property cardmend doesn't interpret, kept so writing the contact back
/// out loses nothing (IMPP, X-SOCIALPROFILE, RELATED, and so on).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Extra {
    pub group: Option<String>,
    pub name: String,
    pub params: Vec<(String, String)>,
    pub value: String,
}

/// Where a contact came from, for error messages and the review page.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Source {
    pub file: String,
    /// Position of the contact within its file, from 0.
    pub index: usize,
    /// Line of the file where the contact starts, from 1.
    pub line: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Contact {
    /// Position in the combined address book; set by `Book`.
    pub id: usize,
    pub source: Source,
    pub formatted_name: String,
    pub name: Name,
    pub nicknames: Vec<String>,
    pub org: String,
    pub department: String,
    pub title: String,
    pub phones: Vec<Field<String>>,
    pub emails: Vec<Field<String>>,
    pub addresses: Vec<Field<Address>>,
    pub urls: Vec<Field<String>>,
    /// As written in the file: `1984-03-07`, `--03-07`, `19840307`...
    #[serde(skip_serializing_if = "Option::is_none")]
    pub birthday: Option<String>,
    pub note: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub photo: Option<Photo>,
    pub categories: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uid: Option<String>,
    pub extra: Vec<Extra>,
}

impl Contact {
    /// The name to show: FN, else N, else the organisation.
    pub fn display_name(&self) -> String {
        let fname = self.formatted_name.trim();
        if !fname.is_empty() {
            return fname.to_string();
        }
        let n = self.name.display();
        if !n.is_empty() {
            return n;
        }
        self.org.trim().to_string()
    }

    pub fn has_name(&self) -> bool {
        !self.display_name().is_empty()
    }

    /// Nothing worth keeping: no name, number, email, address or note.
    pub fn is_empty(&self) -> bool {
        !self.has_name()
            && self.phones.iter().all(|p| p.value.trim().is_empty())
            && self.emails.iter().all(|e| e.value.trim().is_empty())
            && self.addresses.iter().all(|a| a.value.is_empty())
            && self.urls.is_empty()
            && self.note.trim().is_empty()
            && self.birthday.is_none()
            && self.photo.is_none()
    }
}

/// Dates in the one form cardmend writes: `YYYY-MM-DD`, or `--MM-DD` with no
/// year. Accepts the forms exporters use (`19840307`, `--0307`, a trailing
/// time, Outlook's `3/7/1984`). Returns None for empty and Outlook's
/// `0/0/00` placeholder; anything else unrecognised is kept as written.
pub fn clean_date(raw: &str) -> Option<String> {
    let s = raw.trim();
    let s = s.split('T').next().unwrap_or(s);
    if s.is_empty() || s.chars().all(|c| matches!(c, '0' | '/' | '-' | '.')) {
        return None;
    }
    let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    let valid = |m: &str, d: &str| {
        matches!(m.parse::<u32>(), Ok(1..=12)) && matches!(d.parse::<u32>(), Ok(1..=31))
    };
    if let Some(rest) = s.strip_prefix("--") {
        let md: String = rest.chars().filter(|c| c.is_ascii_digit()).collect();
        if md.len() == 4 && valid(&md[..2], &md[2..]) {
            return Some(format!("--{}-{}", &md[..2], &md[2..]));
        }
    } else if s.contains('/') {
        // Outlook writes month/day/year.
        let p: Vec<&str> = s.split('/').collect();
        if let [m, d, y] = p[..]
            && y.len() == 4
            && valid(m, d)
        {
            return Some(format!("{y}-{m:0>2}-{d:0>2}"));
        }
    } else if digits.len() == 8 && valid(&digits[4..6], &digits[6..]) {
        return Some(format!(
            "{}-{}-{}",
            &digits[..4],
            &digits[4..6],
            &digits[6..]
        ));
    }
    Some(s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(clean_date("1984-03-07").as_deref(), Some("1984-03-07"));
        assert_eq!(clean_date("19840307").as_deref(), Some("1984-03-07"));
        assert_eq!(
            clean_date("1984-03-07T00:00:00Z").as_deref(),
            Some("1984-03-07")
        );
        assert_eq!(clean_date("--0307").as_deref(), Some("--03-07"));
        assert_eq!(clean_date("--03-07").as_deref(), Some("--03-07"));
        assert_eq!(clean_date("3/7/1984").as_deref(), Some("1984-03-07"));
        assert_eq!(clean_date("0/0/00"), None);
        assert_eq!(clean_date(" "), None);
        assert_eq!(clean_date("spring 1990").as_deref(), Some("spring 1990"));
    }
}
