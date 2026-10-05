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

use phonenumber::country::Id;
use serde::Serialize;

use crate::cluster::Group;
use crate::contact::Contact;
use crate::matching::{Pair, Shared};

/// Things worth fixing that aren't duplicates.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Problems {
    /// Contacts with details but no name or company.
    pub no_name: Vec<usize>,
    /// Numbers saved without a country code: (contact id, number as saved).
    /// They break when you travel or message abroad.
    pub no_country: Vec<(usize, String)>,
    /// Entries with nothing in them; left out of the clean file.
    pub empty: Vec<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Analysis {
    pub pairs: Vec<Pair>,
    pub groups: Vec<Group>,
    pub shared: Vec<Shared>,
    pub problems: Problems,
}

/// Find duplicate groups and other problems. `not_duplicates` are pairs of
/// contact ids the user said are different people.
pub fn analyze(contacts: &[Contact], region: Id, not_duplicates: &[(usize, usize)]) -> Analysis {
    let prep: Vec<_> = contacts
        .iter()
        .map(|c| matching::prepare(c, region))
        .collect();
    let shared = matching::find_shared(&prep);
    let pairs = matching::pairs(&prep, &shared);
    let groups = cluster::cluster(
        contacts.len(),
        &pairs,
        |a, b| matching::conflict(&prep[a], &prep[b]),
        not_duplicates,
    );
    let mut problems = Problems::default();
    for c in contacts {
        if c.is_empty() {
            problems.empty.push(c.id);
            continue;
        }
        if !c.has_name() {
            problems.no_name.push(c.id);
        }
        for f in &c.phones {
            if normalize::phone(&f.value, region).is_some_and(|n| !n.has_country) {
                problems.no_country.push((c.id, f.value.clone()));
            }
        }
    }
    Analysis {
        pairs,
        groups,
        shared,
        problems,
    }
}
