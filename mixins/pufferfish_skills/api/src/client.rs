//! The client half: the key, the window, its clicks, and the state the server sent.

use std::sync::Mutex;

use horizon_client::mod_api::*;

use crate::gui;
use crate::net::{self, Request, Snapshot};
use crate::{CHANNEL_ACTION, KEY_DEFAULT, KEY_NAME, MOD_ID};

/// What the window draws and sends. The server owns the truth; this is its last copy.
#[derive(Default)]
pub struct Client {
    view: Mutex<View>,
}

#[derive(Default)]
struct View {
    snapshot: Option<Snapshot>,
    /// The window in GUI pixels, as the last frame's screen hook gave it.
    viewport: [f32; 4],
    /// Where the pointer was that frame; a click lands where the pointer already is.
    mouse: [f32; 2],
    category: usize,
}

/// What a click asked for.
enum Action {
    Close,
    Request(Request),
}

impl Client {
    /// Whether the open screen is this mod's own.
    fn mine(host: &dyn Host) -> bool {
        host.mod_screen() == Some(MOD_ID)
    }

    /// Opens the tree, and asks the server for the player's progress.
    pub fn open(&self, host: &mut dyn Host) {
        if host.mod_screen_open(MOD_ID) {
            let _ = host.net_send_to_server(CHANNEL_ACTION, b"sync");
        }
    }

    pub fn key(&self, host: &mut dyn Host, key: &str, pressed: bool) {
        if !pressed || !host.playing() {
            return;
        }
        if key == host.mod_key(MOD_ID, KEY_NAME, KEY_DEFAULT) {
            self.open(host);
        }
    }

    /// `/skills`, typed or clicked in chat: the tree, from anywhere.
    pub fn command(&self, host: &mut dyn Host, line: &str) -> bool {
        let line = line.trim();
        if line == "skills" || line == "skills tree" {
            self.open(host);
            return true;
        }
        false
    }

    /// The commands the chat box colours and completes as this mod's.
    pub fn commands(&self, commands: &mut Vec<&'static str>) {
        commands.push("skills");
    }

    /// Options > Mods lists the mod and its key.
    pub fn mods(&self, mods: &mut Vec<NativeMod>) {
        mods.push(NativeMod {
            id: MOD_ID.to_owned(),
            title: "Pufferfish's Skills".to_owned(),
            description: "A permanent skill tree: earn experience, spend points on attribute \
                          rewards. The tree opens with its key or /skills."
                .to_owned(),
            keys: vec![[
                KEY_NAME.to_owned(),
                "Open skill tree".to_owned(),
                KEY_DEFAULT.to_owned(),
            ]],
        });
    }

    /// The server sent the player's progress.
    pub fn message(&self, data: &[u8]) {
        let Ok(text) = std::str::from_utf8(data) else {
            return;
        };
        let Some(snapshot) = net::decode_snapshot(text) else {
            eprintln!("pufferfish_skills: a malformed state message");
            return;
        };
        if let Ok(mut view) = self.view.lock() {
            view.snapshot = Some(snapshot);
        }
    }

    /// Draws the tree over the open screen.
    pub fn render(&self, host: &dyn Host, batch: &mut HudBatch, screen: &NativeScreenView<'_>) {
        if screen.id != "mod" || !Self::mine(host) {
            return;
        }
        let Ok(mut view) = self.view.lock() else {
            return;
        };
        view.viewport = screen.rect;
        view.mouse = screen.mouse;
        let state = view
            .snapshot
            .as_ref()
            .map(Snapshot::state)
            .unwrap_or_default();
        let notice = view.snapshot.as_ref().and_then(|s| s.notice.clone());
        gui::draw(
            host,
            batch,
            screen.rect,
            screen.mouse,
            crate::tree(),
            &state,
            view.category,
            notice.as_deref(),
        );
    }

    /// A mouse button over the mod's own screen: the tree's clicks. True when the click is the
    /// mod's (it never is once the screen is gone).
    pub fn mouse(&self, host: &mut dyn Host, button: i32, pressed: bool) -> bool {
        if !Self::mine(host) || !pressed {
            return false;
        }
        if button != 0 {
            return true;
        }
        let at = host.cursor();
        let action = {
            let Ok(mut view) = self.view.lock() else {
                return true;
            };
            let state = view
                .snapshot
                .as_ref()
                .map(Snapshot::state)
                .unwrap_or_default();
            let layout = gui::layout(view.viewport, crate::tree(), &state, view.category, at);
            if layout.close_at(at) {
                Some(Action::Close)
            } else if let Some(tab) = layout.tab_at(at) {
                view.category = tab;
                None
            } else {
                layout.node_at(at).and_then(|index| {
                    let category = crate::tree().categories.get(view.category)?;
                    let node = category.nodes.get(index)?;
                    Some(Action::Request(Request::Unlock {
                        category: category.id.clone(),
                        node: node.id.clone(),
                    }))
                })
            }
        };
        match action {
            Some(Action::Close) => host.mod_screen_close(MOD_ID),
            Some(Action::Request(request)) => {
                let text = net::encode_request(&request);
                if let Err(e) = host.net_send_to_server(CHANNEL_ACTION, text.as_bytes()) {
                    eprintln!("pufferfish_skills: could not ask the server: {e}");
                }
            }
            None => {}
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CHANNEL_STATE;

    /// The state the server sends names the categories the tree has.
    #[test]
    fn a_state_round_trips_through_the_channel() {
        let mut state = crate::state::PlayerState::default();
        state.add_xp(crate::tree(), 300);
        let text = net::encode_snapshot(&Snapshot::of(&state).with_notice("hello"));
        let client = Client::default();
        client.message(text.as_bytes());
        let view = client.view.lock().unwrap();
        let snapshot = view.snapshot.as_ref().expect("a snapshot");
        assert_eq!(snapshot.state(), state);
        assert_eq!(snapshot.notice.as_deref(), Some("hello"));
    }

    #[test]
    fn the_channel_matches_the_mod_id() {
        assert!(CHANNEL_STATE.starts_with(MOD_ID));
        assert!(CHANNEL_ACTION.starts_with(MOD_ID));
    }
}
