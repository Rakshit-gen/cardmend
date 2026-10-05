use cardmend::contact::Photo;
use cardmend::vcard;

fn read(
    name: &str,
) -> (
    Vec<cardmend::contact::Contact>,
    Vec<cardmend::import::Issue>,
) {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let text = cardmend::import::decode_text(&std::fs::read(path).unwrap());
    vcard::read(&text, name)
}

#[test]
fn icloud_export() {
    let (cs, issues) = read("icloud.vcf");
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(cs.len(), 2);
    let c = &cs[0];
    assert_eq!(c.formatted_name, "Dr. Robert Smith");
    assert_eq!(c.name.given, "Robert");
    assert_eq!(c.name.prefix, "Dr.");
    assert_eq!(c.org, "Acme Logistics");
    assert_eq!(c.department, "Operations");
    assert_eq!(c.emails[0].value, "rob.smith@acme-logistics.example");
    assert_eq!(c.emails[0].types, ["pref"]);
    assert_eq!(c.phones[0].types, ["cell", "voice", "pref"]);
    assert_eq!(c.phones[0].label.as_deref(), Some("Mobile"));
    assert_eq!(c.phones[1].label.as_deref(), Some("Office"));
    assert_eq!(c.addresses[0].value.locality, "San Francisco");
    assert_eq!(c.birthday.as_deref(), Some("--03-07"));
    assert_eq!(
        c.note,
        "Met at the 2019 conference, ask about the boat.\nLikes sailing."
    );
    match &c.photo {
        Some(Photo::Data { mime, bytes }) => {
            assert_eq!(mime, "image/jpeg");
            assert_eq!(bytes.len(), 160);
        }
        other => panic!("{other:?}"),
    }
    // The anniversary and its label survive as extras with their group.
    let names: Vec<_> = c.extra.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"X-ABDATE"));
    assert!(names.contains(&"X-ABADR"));
    assert!(names.contains(&"X-SOCIALPROFILE"));
    assert!(
        c.extra
            .iter()
            .any(|e| e.name == "X-ABLABEL" && e.value == "Anniversary")
    );
    assert_eq!(c.source.line, 1);
    assert_eq!(cs[1].source.line, 24);
    assert_eq!(cs[1].display_name(), "Acme Logistics");
}

#[test]
fn google_export() {
    let (cs, issues) = read("google.vcf");
    assert!(issues.is_empty(), "{issues:?}");
    let c = &cs[0];
    assert_eq!(c.nicknames, ["Lizzie"]);
    assert_eq!(c.phones[1].label.as_deref(), Some("Grandma's"));
    assert_eq!(c.urls[0].value, "https://liztaylor.example");
    assert_eq!(c.urls[0].label.as_deref(), Some("blog"));
    assert_eq!(
        c.photo,
        Some(Photo::Uri(
            "https://lh3.googleusercontent.com/contacts/AG6tpzExample".into()
        ))
    );
    assert_eq!(c.categories, ["myContacts", "Family"]);
}

#[test]
fn android_21_export() {
    let (cs, issues) = read("android21.vcf");
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(cs.len(), 2);
    let c = &cs[0];
    assert_eq!(c.formatted_name, "Jürgen Müller");
    assert_eq!(c.name.family, "Müller");
    assert_eq!(c.phones[1].types, ["home", "voice"]);
    assert_eq!(
        c.note,
        "Line one\nLine two is long enough that Android breaks it"
    );
    assert_eq!(c.photo.as_ref().unwrap().size(), 160);
    assert_eq!(c.extra[0].name, "X-ANDROID-CUSTOM");
    // ISO-8859-1 inside quoted-printable.
    assert_eq!(cs[1].name.given, "José");
    assert_eq!(cs[1].name.family, "García");
    assert_eq!(cs[1].display_name(), "José García");
    assert_eq!(cs[1].phones[0].types, ["pref", "cell"]);
}

#[test]
fn vcard_40() {
    let (cs, issues) = read("v4.vcf");
    assert!(issues.is_empty(), "{issues:?}");
    let c = &cs[0];
    assert_eq!(c.phones[0].value, "+34-612-345-678");
    assert_eq!(c.phones[0].types, ["voice", "cell"]);
    assert_eq!(c.birthday.as_deref(), Some("--04-12"));
    match &c.photo {
        Some(Photo::Data { mime, bytes }) => {
            assert_eq!(mime, "image/png");
            assert_eq!(&bytes[1..4], b"PNG");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn reports_broken_structure_with_line_numbers() {
    let text = "TEL:1\nBEGIN:VCARD\nFN:A\nthis is junk\nBEGIN:VCARD\nFN:B\nPHOTO;ENCODING=b:@@@@\nEND:VCARD\nBEGIN:VCARD\nFN:C\n";
    let (cs, issues) = vcard::read(text, "broken.vcf");
    let names: Vec<_> = cs.iter().map(|c| c.formatted_name.as_str()).collect();
    assert_eq!(names, ["A", "B", "C"]);
    let lines: Vec<_> = issues.iter().map(|i| i.line).collect();
    assert_eq!(lines, [1, 4, 2, 7, 9]);
    assert!(issues[3].message.contains("photo"));
}
