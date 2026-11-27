//! The sync decision, kept free of files and network so every case can be tested.
//!
//! Three things are compared: the save on the device, the newest compatible save on
//! RomM, and the record of what the two were the last time they were in sync. Only one
//! side changed → that side wins. Both changed → the user decides. Nothing here ever
//! picks a side when both changed.

use serde::{Deserialize, Serialize};

/// The device's save.
#[derive(Debug, Clone, PartialEq)]
pub struct Local {
    /// Fingerprint that ignores files emulators rewrite without new progress
    /// (PCSX2's `_pcsx2_index`).
    pub fp: String,
    /// Hash computed the way RomM computes `content_hash`.
    pub hash: String,
}

/// The newest compatible save on the server.
#[derive(Debug, Clone, PartialEq)]
pub struct Server {
    pub id: i64,
    pub hash: Option<String>,
}

/// What both sides were when they were last in sync.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub local_fp: Option<String>,
    pub server_id: Option<i64>,
    pub server_hash: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Neither side has a save.
    Nothing,
    /// Same content on both sides (the record may still need updating).
    InSync,
    Upload,
    Download,
    /// Both changed since the last sync, or they differ and were never synced.
    Conflict,
}

fn server_changed(s: &Server, r: &Record) -> bool {
    let same_id = r.server_id == Some(s.id);
    let same_hash = s.hash.is_some() && s.hash == r.server_hash;
    !(same_id || same_hash)
}

pub fn decide(local: Option<&Local>, server: Option<&Server>, record: Option<&Record>) -> Action {
    match (local, server) {
        (None, None) => Action::Nothing,
        (Some(_), None) => Action::Upload,
        // Never synced, or deleted on the device: restoring is the safe direction.
        (None, Some(_)) => Action::Download,
        (Some(l), Some(s)) => {
            if s.hash.as_deref() == Some(l.hash.as_str()) {
                return Action::InSync;
            }
            let Some(r) = record else {
                return Action::Conflict;
            };
            let local_changed = r.local_fp.as_deref() != Some(l.fp.as_str());
            match (local_changed, server_changed(s, r)) {
                (false, false) => Action::InSync,
                (true, false) => Action::Upload,
                (false, true) => Action::Download,
                (true, true) => Action::Conflict,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Action::*;

    fn l(fp: &str, hash: &str) -> Local {
        Local {
            fp: fp.into(),
            hash: hash.into(),
        }
    }
    fn s(id: i64, hash: &str) -> Server {
        Server {
            id,
            hash: Some(hash.into()),
        }
    }
    fn r(fp: &str, id: i64, hash: &str) -> Record {
        Record {
            local_fp: Some(fp.into()),
            server_id: Some(id),
            server_hash: Some(hash.into()),
        }
    }

    /// (name, device, server, last sync, expected)
    type Case = (&'static str, Option<Local>, Option<Server>, Option<Record>, Action);

    #[test]
    fn every_case() {
        let rec = r("fpA", 1, "hA");
        #[rustfmt::skip]
        let cases: Vec<Case> = vec![
            ("nothing anywhere",                   None,                  None,             None,              Nothing),
            ("only on the device",                 Some(l("fpA", "hA")),  None,             None,              Upload),
            ("only on the device, synced before",  Some(l("fpA", "hA")),  None,             Some(rec.clone()), Upload),
            ("only on the server",                 None,                  Some(s(1, "hA")), None,              Download),
            ("deleted on the device",              None,                  Some(s(1, "hA")), Some(rec.clone()), Download),
            ("same content, never synced",         Some(l("fpX", "hA")),  Some(s(1, "hA")), None,              InSync),
            ("different, never synced",            Some(l("fpB", "hB")),  Some(s(1, "hA")), None,              Conflict),
            ("nothing changed",                    Some(l("fpA", "hA2")), Some(s(1, "hA")), Some(rec.clone()), InSync),
            ("played on the device",               Some(l("fpB", "hB")),  Some(s(1, "hA")), Some(rec.clone()), Upload),
            ("played elsewhere",                   Some(l("fpA", "hX")),  Some(s(2, "hC")), Some(rec.clone()), Download),
            ("played on both",                     Some(l("fpB", "hB")),  Some(s(2, "hC")), Some(rec.clone()), Conflict),
            ("re-uploaded same content (new id)",  Some(l("fpA", "hX")),  Some(s(9, "hA")), Some(rec.clone()), InSync),
            ("index-only change on the device",    Some(l("fpA", "hA3")), Some(s(1, "hA")), Some(rec.clone()), InSync),
        ];
        for (name, local, server, record, want) in cases {
            assert_eq!(
                decide(local.as_ref(), server.as_ref(), record.as_ref()),
                want,
                "{name}"
            );
        }
    }

    #[test]
    fn server_without_hash_compares_by_id() {
        let rec = Record {
            local_fp: Some("fp".into()),
            server_id: Some(1),
            server_hash: None,
        };
        let same = Server { id: 1, hash: None };
        let newer = Server { id: 2, hash: None };
        assert_eq!(decide(Some(&l("fp", "h")), Some(&same), Some(&rec)), InSync);
        assert_eq!(
            decide(Some(&l("fp", "h")), Some(&newer), Some(&rec)),
            Download
        );
    }
}
