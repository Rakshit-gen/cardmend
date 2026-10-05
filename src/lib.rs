//! cardmend: find the duplicates in an address book exported from phones and
//! mail services, explain each match, and merge them into one clean file.
pub mod cluster;
pub mod contact;
pub mod csv_import;
pub mod import;
pub mod matching;
pub mod merge;
pub mod normalize;
pub mod vcard;
pub mod write;
