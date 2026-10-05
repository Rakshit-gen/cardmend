//! Reading a file, writing it as 3.0 and reading that back must give the
//! same contacts. Only where a contact came from (file, line) may differ,
//! and a missing FN, which 3.0 requires, is filled in from N or ORG.

use cardmend::contact::Contact;
use cardmend::{import, vcard, write};

fn without_source(mut cs: Vec<Contact>) -> Vec<Contact> {
    for c in &mut cs {
        c.source = Default::default();
        c.formatted_name = c.display_name();
    }
    cs
}

fn check(name: &str) {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let text = import::decode_text(&std::fs::read(path).unwrap());
    let (first, issues) = vcard::read(&text, name);
    assert!(issues.is_empty(), "{issues:?}");
    let out = write::write_all(&first);
    let (second, issues) = vcard::read(&out, "out.vcf");
    assert!(issues.is_empty(), "{issues:?}\n{out}");
    assert_eq!(without_source(first), without_source(second), "\n{out}");
}

#[test]
fn icloud() {
    check("icloud.vcf");
}

#[test]
fn google() {
    check("google.vcf");
}

#[test]
fn android_21() {
    check("android21.vcf");
}

#[test]
fn vcard_40() {
    check("v4.vcf");
}

#[test]
fn awkward_values_survive() {
    let text = "BEGIN:VCARD\r\nVERSION:3.0\r\nN:O\\;Brien;Seán;;;\r\nFN:Seán O;Brien\r\n\
        NOTE:a\\, b\\; c \\\\ d\\nnext line with ünïcödé and a very long tail that has to be folded more than once to fit\r\n\
        CATEGORIES:x\\,y,z\r\nTEL;TYPE=\"cell,voice\":+353 87 123 4567\r\nEND:VCARD\r\n";
    let (first, _) = vcard::read(text, "a.vcf");
    let out = write::write_all(&first);
    let (second, _) = vcard::read(&out, "b.vcf");
    assert_eq!(without_source(first.clone()), without_source(second));
    assert_eq!(first[0].name.family, "O;Brien");
    assert_eq!(first[0].categories, ["x,y", "z"]);
}
