//! Pufferfish's Skills, for Horizon.
//!
//! A clean-room reimplementation of the *behaviour* of Pufferfish's Skills (a permanent skill
//! tree with experience-earned points and attribute rewards): the original mod ships under a
//! custom licence that forbids redistribution, so none of its code, data or textures is used here.
//! The tree, the points, the rewards and the window are this mod's own, from the public
//! description of how such a tree works.
//!
//! Hook points used (docs/native-mods.md, "Trait mods"): a player joining and leaving, killing,
//! mining, an advancement, a catch, a craft and a breeding (`mob_killed`, `destroy_block`,
//! `player_advancement_done`, `player_fish`, `item_crafted`, `baby_entity_spawn`), the two halves'
//! messages (`net_from_client`, `net_from_server`), a server's commands (`register_commands`,
//! `perform_command`), the client's own (`commands`, `command`), the key (`key`, `mods`) and the
//! tree's window (`screen_render_post`, `mouse_button_pre`).
#![forbid(unsafe_code)]

use std::sync::LazyLock;

use horizon_client::mod_api::*;

mod client;
mod gui;
mod net;
mod server;
mod state;
mod store;
mod skill_tree;

/// The mod's id: its settings key, its files, its channels and its point in a panic.
pub const MOD_ID: &str = "pufferfish_skills";
/// The channel the server sends a player's progress on.
pub const CHANNEL_STATE: &str = "pufferfish_skills:state";
/// The channel the client asks on.
pub const CHANNEL_ACTION: &str = "pufferfish_skills:action";
/// The key's name in Options > Mods, and the key it starts as.
pub const KEY_NAME: &str = "open_tree";
pub const KEY_DEFAULT: &str = "key.keyboard.k";

/// The tree both halves read: `data/tree.json`, compiled into the mod. Read once, on the first
/// hook that needs it.
pub fn tree() -> &'static skill_tree::Tree {
    static TREE: LazyLock<skill_tree::Tree> =
        LazyLock::new(|| match skill_tree::parse(skill_tree::DEFAULT_TREE) {
            Ok(tree) => tree,
            Err(e) => {
                eprintln!("pufferfish_skills: data/tree.json: {e}");
                skill_tree::Tree::empty()
            }
        });
    &TREE
}

/// The launcher calls this for the built game.
pub fn hooks() -> Box<dyn Hooks> {
    Box::new(Skills::default())
}

/// One instance serves the window's game and the server thread alike, so what it keeps lives
/// behind locks (`Hooks` is `&self` and `Send + Sync`).
#[derive(Default)]
struct Skills {
    client: client::Client,
    server: server::Server,
}

impl Hooks for Skills {
    fn player_logged_in(&self, host: &mut dyn Host, event: &mut PlayerLoggedInEvent) {
        self.server.logged_in(host, event.player, event.name);
    }

    fn player_logged_out(&self, host: &mut dyn Host, event: &mut PlayerLoggedOutEvent) {
        self.server.logged_out(host, event.player);
    }

    fn net_from_client(&self, host: &mut dyn Host, event: &mut NetFromClientEvent) {
        self.server
            .message(host, event.player, event.channel, event.data);
    }

    fn net_from_server(&self, host: &mut dyn Host, event: &mut NetFromServerEvent<'_>) {
        let _ = host;
        if event.channel == CHANNEL_STATE {
            self.client.message(event.data);
        }
    }

    fn mob_killed(&self, host: &mut dyn Host, event: &mut MobKilledEvent<'_>) {
        self.server.mob_killed(host, event.player, event.kind);
    }

    fn destroy_block(&self, host: &mut dyn Host, event: &mut DestroyBlockEvent<'_>) -> Flow {
        self.server.block_broken(host, event.pos);
        Flow::Continue
    }

    fn player_advancement_done(
        &self,
        host: &mut dyn Host,
        event: &mut PlayerAdvancementDoneEvent<'_>,
    ) {
        self.server
            .advancement_done(host, event.player, event.advancement);
    }

    fn player_fish(&self, host: &mut dyn Host, event: &mut PlayerFishEvent<'_>) -> Flow {
        self.server.fished(host, event.player, event.state);
        Flow::Continue
    }

    fn item_crafted(&self, host: &mut dyn Host, event: &mut ItemCraftedEvent<'_>) {
        self.server.crafted(host, event.player, event.recipe);
    }

    fn baby_entity_spawn(
        &self,
        host: &mut dyn Host,
        event: &mut BabyEntitySpawnEvent<'_>,
    ) -> Flow {
        self.server.bred(host, event.player, event.kind);
        Flow::Continue
    }

    fn register_commands(&self, host: &mut dyn Host, event: &mut RegisterCommandsEvent) {
        let _ = host;
        self.server.register(event.commands);
    }

    fn perform_command(&self, host: &mut dyn Host, event: &mut PerformCommandEvent<'_>) -> Flow {
        if self.server.command(host, event.player, event.command) {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    fn mods(&self, host: &mut dyn Host, event: &mut ModsEvent<'_>) {
        let _ = host;
        self.client.mods(event.mods);
    }

    fn key(&self, host: &mut dyn Host, event: &mut KeyEvent<'_>) {
        self.client.key(host, event.key, event.pressed);
    }

    fn commands(&self, host: &mut dyn Host, event: &mut CommandsEvent) {
        let _ = host;
        self.client.commands(event.commands);
    }

    fn command(&self, host: &mut dyn Host, event: &mut CommandEvent<'_>) -> Flow {
        if self.client.command(host, event.command) {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    fn screen_render_post(&self, host: &dyn Host, event: &mut ScreenRenderPostEvent<'_>) {
        self.client.render(host, event.batch, event.screen);
    }

    fn mouse_button_pre(&self, host: &mut dyn Host, event: &mut MouseButtonPreEvent) -> Flow {
        if self.client.mouse(host, event.button, event.pressed) {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }
}
