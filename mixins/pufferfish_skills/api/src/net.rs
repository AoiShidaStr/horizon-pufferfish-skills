//! The bytes between the mod's two halves, on Horizon's `horizon:mod_net` channel.
//!
//! Plain text, one fact a line: no dependency, and a player's client never gets more than the
//! state it draws. A client's messages are treated as the player's own input: the server checks
//! every request against its own tree and its own copy of the player's progress.

use std::collections::{BTreeMap, BTreeSet};

use crate::state::{PlayerState, Progress};

/// The server's answer: a player's whole progress, and a line to show them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub categories: BTreeMap<String, Progress>,
    pub unlocked: BTreeSet<String>,
    /// What the server wants the player told (a refusal, or a skill unlocked).
    pub notice: Option<String>,
}

impl Snapshot {
    pub fn of(state: &PlayerState) -> Self {
        Self {
            categories: state.categories.clone(),
            unlocked: state.unlocked.clone(),
            notice: None,
        }
    }

    /// The state a snapshot was made of.
    pub fn state(&self) -> PlayerState {
        PlayerState {
            categories: self.categories.clone(),
            unlocked: self.unlocked.clone(),
        }
    }

    pub fn with_notice(mut self, notice: impl Into<String>) -> Self {
        self.notice = Some(notice.into());
        self
    }
}

/// What a client asks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// Send my progress: the GUI opened.
    Sync,
    Unlock { category: String, node: String },
    Reset { category: String },
}

/// `cat <id> <xp> <spent>`, `un <category> <node>`, `notice <free text>`, then `end`.
pub fn encode_snapshot(snapshot: &Snapshot) -> String {
    let mut text = String::new();
    for (id, progress) in &snapshot.categories {
        text.push_str(&format!("cat {id} {} {}\n", progress.xp, progress.spent));
    }
    for key in &snapshot.unlocked {
        text.push_str(&format!("un {key}\n"));
    }
    if let Some(notice) = &snapshot.notice {
        let notice = notice.replace('\n', " ");
        text.push_str(&format!("notice {notice}\n"));
    }
    text.push_str("end\n");
    text
}

pub fn decode_snapshot(text: &str) -> Option<Snapshot> {
    let mut snapshot = Snapshot::default();
    let mut ended = false;
    for line in text.lines() {
        if line == "end" {
            ended = true;
            break;
        }
        let (kind, rest) = line.split_once(' ')?;
        match kind {
            "cat" => {
                let mut parts = rest.split(' ');
                let id = parts.next()?.to_owned();
                let xp = parts.next()?.parse().ok()?;
                let spent = parts.next()?.parse().ok()?;
                snapshot
                    .categories
                    .insert(id, Progress { xp, spent });
            }
            "un" => {
                snapshot.unlocked.insert(rest.to_owned());
            }
            "notice" => snapshot.notice = Some(rest.to_owned()),
            _ => return None,
        }
    }
    ended.then_some(snapshot)
}

pub fn encode_request(request: &Request) -> String {
    match request {
        Request::Sync => "sync".to_owned(),
        Request::Unlock { category, node } => format!("unlock {category} {node}"),
        Request::Reset { category } => format!("reset {category}"),
    }
}

pub fn decode_request(text: &str) -> Option<Request> {
    let text = text.trim();
    if text == "sync" {
        return Some(Request::Sync);
    }
    let (kind, rest) = text.split_once(' ')?;
    let (category, node) = match rest.split_once(' ') {
        Some((category, node)) => (category, Some(node)),
        None => (rest, None),
    };
    if !crate::skill_tree::plain_id(category) {
        return None;
    }
    match (kind, node) {
        ("unlock", Some(node)) if crate::skill_tree::plain_id(node) => Some(Request::Unlock {
            category: category.to_owned(),
            node: node.to_owned(),
        }),
        ("reset", None) => Some(Request::Reset {
            category: category.to_owned(),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> Snapshot {
        let mut snapshot = Snapshot::default();
        snapshot.categories.insert(
            "adventure".into(),
            Progress {
                xp: 240,
                spent: 1,
            },
        );
        snapshot.unlocked.insert("adventure/vitality_1".into());
        snapshot
    }

    #[test]
    fn a_snapshot_survives_a_round_trip() {
        let snapshot = snapshot();
        let text = encode_snapshot(&snapshot);
        assert_eq!(decode_snapshot(&text), Some(snapshot));
    }

    #[test]
    fn a_notice_keeps_its_spaces() {
        let snapshot = snapshot().with_notice("that skill costs 2 points, you have 1");
        let decoded = decode_snapshot(&encode_snapshot(&snapshot)).unwrap();
        assert_eq!(
            decoded.notice.as_deref(),
            Some("that skill costs 2 points, you have 1")
        );
    }

    #[test]
    fn a_truncated_snapshot_is_refused() {
        assert_eq!(decode_snapshot("cat adventure 1 2\n"), None);
        assert_eq!(decode_snapshot("nonsense\nend\n"), None);
        assert_eq!(decode_snapshot(""), None);
    }

    #[test]
    fn requests_round_trip() {
        for request in [
            Request::Sync,
            Request::Unlock {
                category: "adventure".into(),
                node: "vitality_1".into(),
            },
            Request::Reset {
                category: "gathering".into(),
            },
        ] {
            assert_eq!(decode_request(&encode_request(&request)), Some(request));
        }
    }

    #[test]
    fn a_bad_request_is_refused() {
        assert_eq!(decode_request("unlock adventure"), None);
        assert_eq!(decode_request("unlock Adventure vitality_1"), None);
        assert_eq!(decode_request("reset"), None);
        assert_eq!(decode_request("give me points"), None);
    }
}
