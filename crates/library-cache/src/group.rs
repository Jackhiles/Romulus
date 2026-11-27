//! One list entry per game.
//!
//! RomM links entries of the same game as siblings. Two kinds show up:
//! - multi-disc games stored as loose files: a `.m3u` playlist plus one entry per disc
//!   image. The playlist is the game; the discs are its parts (hidden, and downloaded
//!   with it).
//! - alternative versions (regions, revisions, hacks): one represents the game in lists,
//!   the others are reachable from its page.

use std::collections::HashMap;

const DISC_EXT: &[&str] = &[
    "chd", "cue", "bin", "iso", "img", "ccd", "sub", "mds", "mdf", "pbp", "cso", "gdi", "rvz",
];

#[derive(Debug, Clone)]
pub struct Member {
    pub id: i64,
    pub fs_name: String,
    pub siblings: Vec<i64>,
    /// RomM's "main sibling" choice for this user.
    pub main: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Group {
    /// The entry shown in lists.
    pub rep: i64,
    /// Other versions of the game (shown on its page).
    pub versions: Vec<i64>,
    /// Disc images played through the representative's playlist.
    pub parts: Vec<i64>,
}

fn ext(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default()
}

/// Higher is better: region, then a release over betas/demos, then later revisions,
/// then the shorter name.
fn score(name: &str) -> (i32, i32, i32, i32) {
    let n = name.to_ascii_lowercase();
    let region = if n.contains("usa") || n.contains("(u)") {
        3
    } else if n.contains("world") {
        2
    } else if n.contains("europe") || n.contains("(e)") {
        1
    } else {
        0
    };
    let release = if ["(beta", "(proto", "(demo", "(sample", "(hack", "[t-"]
        .iter()
        .any(|t| n.contains(t))
    {
        0
    } else {
        1
    };
    let rev = n
        .split("(rev ")
        .nth(1)
        .and_then(|r| r.split(')').next())
        .and_then(|r| {
            r.trim()
                .parse::<i32>()
                .ok()
                .or_else(|| r.trim().chars().next().map(|c| c as i32 - 'a' as i32 + 1))
        })
        .unwrap_or(0);
    (region, release, rev, -(n.len() as i32))
}

fn best(members: &[&Member]) -> i64 {
    members
        .iter()
        .max_by(|a, b| {
            score(&a.fs_name)
                .cmp(&score(&b.fs_name))
                .then(b.id.cmp(&a.id))
        })
        .map(|m| m.id)
        .unwrap_or_default()
}

/// Group `members` (one platform's entries) by their sibling links.
pub fn group(members: &[Member]) -> Vec<Group> {
    let index: HashMap<i64, usize> = members.iter().enumerate().map(|(i, m)| (m.id, i)).collect();
    let mut parent: Vec<usize> = (0..members.len()).collect();
    fn find(p: &mut [usize], mut x: usize) -> usize {
        while p[x] != x {
            p[x] = p[p[x]];
            x = p[x];
        }
        x
    }
    for (i, m) in members.iter().enumerate() {
        for s in &m.siblings {
            if let Some(&j) = index.get(s) {
                let (a, b) = (find(&mut parent, i), find(&mut parent, j));
                if a != b {
                    parent[a] = b;
                }
            }
        }
    }
    let mut sets: HashMap<usize, Vec<&Member>> = HashMap::new();
    for (i, m) in members.iter().enumerate() {
        let r = find(&mut parent, i);
        sets.entry(r).or_default().push(m);
    }
    let mut out: Vec<Group> = sets
        .into_values()
        .map(|ms| {
            if ms.len() == 1 {
                return Group {
                    rep: ms[0].id,
                    ..Default::default()
                };
            }
            let playlists: Vec<&Member> = ms
                .iter()
                .copied()
                .filter(|m| ext(&m.fs_name) == "m3u")
                .collect();
            let (candidates, parts): (Vec<&Member>, Vec<i64>) = if playlists.is_empty() {
                (ms.clone(), Vec::new())
            } else {
                let parts = ms
                    .iter()
                    .filter(|m| DISC_EXT.contains(&ext(&m.fs_name).as_str()))
                    .map(|m| m.id)
                    .collect();
                (playlists, parts)
            };
            let rep = candidates
                .iter()
                .find(|m| m.main)
                .map(|m| m.id)
                .unwrap_or_else(|| best(&candidates));
            let mut versions: Vec<i64> = ms
                .iter()
                .map(|m| m.id)
                .filter(|id| *id != rep && !parts.contains(id))
                .collect();
            versions.sort_unstable();
            Group {
                rep,
                versions,
                parts,
            }
        })
        .collect();
    out.sort_by_key(|g| g.rep);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(id: i64, name: &str, sib: &[i64]) -> Member {
        Member {
            id,
            fs_name: name.into(),
            siblings: sib.to_vec(),
            main: false,
        }
    }

    #[test]
    fn playlist_hides_its_discs() {
        // Real layout from the Deck's library (docs/DECK_FINDINGS.md).
        let g = group(&[
            m(
                1424,
                "Alone in the Dark - The New Nightmare (USA).m3u",
                &[1425, 1426],
            ),
            m(
                1425,
                "Alone in the Dark - The New Nightmare (USA) (Disc 1).chd",
                &[1424, 1426],
            ),
            m(
                1426,
                "Alone in the Dark - The New Nightmare (USA) (Disc 2).chd",
                &[1424, 1425],
            ),
            m(9, "Other Game (USA).chd", &[]),
        ]);
        assert_eq!(
            g,
            vec![
                Group {
                    rep: 9,
                    ..Default::default()
                },
                Group {
                    rep: 1424,
                    versions: vec![],
                    parts: vec![1425, 1426]
                }
            ]
        );
    }

    #[test]
    fn versions_pick_the_best_or_romms_main() {
        let set = [
            m(1, "Game (Japan).gba", &[2, 3, 4]),
            m(2, "Game (Europe).gba", &[1, 3, 4]),
            m(3, "Game (USA) (Beta).gba", &[1, 2, 4]),
            m(4, "Game (USA) (Rev 1).gba", &[1, 2, 3]),
        ];
        let g = group(&set);
        assert_eq!(g[0].rep, 4);
        assert_eq!(g[0].versions, vec![1, 2, 3]);
        let mut chosen = set.to_vec();
        chosen[0].main = true;
        assert_eq!(group(&chosen)[0].rep, 1);
    }

    #[test]
    fn revision_letters_and_numbers_count() {
        assert!(score("Game (USA) (Rev 2).nes") > score("Game (USA) (Rev 1).nes"));
        assert!(score("Game (USA) (Rev B).nes") > score("Game (USA) (Rev A).nes"));
        assert!(score("Game (USA).nes") > score("Game (Japan).nes"));
    }
}
