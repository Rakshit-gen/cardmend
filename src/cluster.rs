//! Turning scored pairs into groups of contacts.
//!
//! Plain connected components chain: A shares a landline with B, B an email
//! with C, and suddenly a whole family is one contact. Here pairs are joined
//! strongest first, and two groups only join if no contact in one conflicts
//! with any contact in the other (clearly different names, different
//! birthdays, or a pair the user marked as not duplicates). The conflict
//! check runs on every cross pair, not only on scored ones, so a contact
//! with no name can't bridge two different people.

use std::collections::HashSet;

use serde::Serialize;

use crate::matching::{Pair, Tier};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Group {
    /// Contact ids, lowest first.
    pub members: Vec<usize>,
    /// The tier of the weakest pair that joined the group.
    pub tier: Tier,
    pub score: f32,
    /// Indexes into the pair list for every scored pair inside the group,
    /// for showing why.
    pub pairs: Vec<usize>,
}

pub fn cluster(
    n: usize,
    pairs: &[Pair],
    conflict: impl Fn(usize, usize) -> bool,
    not_duplicates: &[(usize, usize)],
) -> Vec<Group> {
    let blocked: HashSet<(usize, usize)> = not_duplicates
        .iter()
        .map(|&(a, b)| (a.min(b), a.max(b)))
        .collect();
    let mut order: Vec<usize> = (0..pairs.len())
        .filter(|&i| Tier::of(pairs[i].score).is_some())
        .collect();
    order.sort_by(|&x, &y| pairs[y].score.total_cmp(&pairs[x].score).then(x.cmp(&y)));

    // Group id per contact, and members and weakest join per group.
    let mut of: Vec<usize> = (0..n).collect();
    let mut members: Vec<Vec<usize>> = (0..n).map(|i| vec![i]).collect();
    let mut weakest: Vec<f32> = vec![f32::INFINITY; n];
    for &pi in &order {
        let p = &pairs[pi];
        let (ga, gb) = (of[p.a], of[p.b]);
        if ga == gb {
            continue;
        }
        let clash = members[ga].iter().any(|&x| {
            members[gb]
                .iter()
                .any(|&y| blocked.contains(&(x.min(y), x.max(y))) || conflict(x, y))
        });
        if clash {
            continue;
        }
        let (keep, gone) = if members[ga].len() >= members[gb].len() {
            (ga, gb)
        } else {
            (gb, ga)
        };
        let moved = std::mem::take(&mut members[gone]);
        for &m in &moved {
            of[m] = keep;
        }
        members[keep].extend(moved);
        weakest[keep] = weakest[keep].min(weakest[gone]).min(p.score);
    }

    let mut groups: Vec<Group> = Vec::new();
    let mut index_of = vec![usize::MAX; n];
    for (g, m) in members.iter_mut().enumerate() {
        if m.len() < 2 {
            continue;
        }
        m.sort_unstable();
        index_of[g] = groups.len();
        groups.push(Group {
            members: m.clone(),
            tier: Tier::of(weakest[g]).expect("joined by a pair that has a tier"),
            score: weakest[g],
            pairs: Vec::new(),
        });
    }
    for (pi, p) in pairs.iter().enumerate() {
        let g = of[p.a];
        if g == of[p.b] && index_of[g] != usize::MAX {
            groups[index_of[g]].pairs.push(pi);
        }
    }
    groups.sort_by(|a, b| a.tier.cmp(&b.tier).then(a.members[0].cmp(&b.members[0])));
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalize::NameMatch;

    fn pair(a: usize, b: usize, score: f32) -> Pair {
        Pair {
            a,
            b,
            score,
            names: NameMatch::Unknown,
            evidence: Vec::new(),
        }
    }

    #[test]
    fn joins_transitively_when_nothing_conflicts() {
        let groups = cluster(4, &[pair(0, 1, 1.1), pair(1, 2, 0.75)], |_, _| false, &[]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].members, [0, 1, 2]);
        assert_eq!(groups[0].tier, Tier::Likely);
        assert_eq!(groups[0].pairs, [0, 1]);
    }

    #[test]
    fn does_not_chain_two_people_through_a_shared_contact() {
        // 1 is a nameless entry with Priya's (0) and Rahul's (2) numbers.
        // 0 and 2 never got scored together, but their names conflict.
        let conflict = |a: usize, b: usize| (a.min(b), a.max(b)) == (0, 2);
        let groups = cluster(3, &[pair(0, 1, 0.6), pair(1, 2, 0.5)], conflict, &[]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].members, [0, 1], "the stronger pair wins");
    }

    #[test]
    fn user_marked_pairs_stay_apart() {
        let groups = cluster(
            3,
            &[pair(0, 1, 1.2), pair(1, 2, 1.1)],
            |_, _| false,
            &[(2, 0)],
        );
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].members, [0, 1]);
    }

    #[test]
    fn sure_groups_come_first() {
        let groups = cluster(4, &[pair(0, 1, 0.5), pair(2, 3, 1.5)], |_, _| false, &[]);
        assert_eq!(groups[0].members, [2, 3]);
        assert_eq!(groups[0].tier, Tier::Sure);
        assert_eq!(groups[1].tier, Tier::Check);
    }
}
