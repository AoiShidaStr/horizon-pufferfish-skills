//! The server half: experience, points, unlocks and the attribute modifiers they give.
//!
//! Everything here runs on the server that owns the world (a dedicated server, or a LAN host),
//! and is checked against the server's own tree and its own copy of the player's progress: a
//! client only ever *asks*.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use horizon_client::mod_api::*;

use crate::net::{self, Request, Snapshot};
use crate::state::{self, PlayerState};
use crate::store;
use crate::skill_tree::{self, Reward};
use crate::{CHANNEL_STATE, MOD_ID};

/// Where a player's progress is while they play.
#[derive(Default)]
pub struct Server {
    players: Mutex<BTreeMap<u128, PlayerState>>,
    /// Attribute commands waiting for the next server tick. `/attribute` only runs outside a
    /// player's hook (the game refuses `run_command` there), and a reward is given inside one:
    /// a click in the tree, or an operator's `/skillsadmin`.
    rewards: Mutex<Vec<String>>,
}

impl Server {
    fn config(&self, host: &dyn Host) -> Option<PathBuf> {
        match host.config_dir() {
            Ok(dir) => Some(dir),
            Err(e) => {
                eprintln!("pufferfish_skills: no config folder: {e}");
                None
            }
        }
    }

    fn state(&self, uuid: u128) -> Option<PlayerState> {
        self.players.lock().ok()?.get(&uuid).cloned()
    }

    /// Writes the player's progress to their file, if they are known.
    fn save(&self, host: &dyn Host, uuid: u128) {
        let (Some(dir), Some(state)) = (self.config(host), self.state(uuid)) else {
            return;
        };
        store::write(&dir, uuid, &state);
    }

    /// Sends the player their progress, with a line to show them.
    fn push(&self, host: &mut dyn Host, uuid: u128, notice: Option<String>) {
        let Some(state) = self.state(uuid) else {
            return;
        };
        let mut snapshot = Snapshot::of(&state);
        snapshot.notice = notice;
        let text = net::encode_snapshot(&snapshot);
        if let Err(e) = host.net_send_to_player(uuid, CHANNEL_STATE, text.as_bytes()) {
            eprintln!("pufferfish_skills: could not send progress to {uuid:032x}: {e}");
        }
    }

    /// The name the attribute command takes for `uuid`.
    fn name_of(host: &dyn Host, uuid: u128) -> Option<String> {
        host.server_players()
            .into_iter()
            .find(|player| player.uuid == uuid)
            .map(|player| player.name)
    }

    /// Queues an attribute command for the next server tick.
    fn queue(&self, line: String) {
        if let Ok(mut lines) = self.rewards.lock() {
            lines.push(line);
        }
    }

    /// Runs the attribute commands queued since the last tick. `server_tick` is a server hook
    /// where `run_command` works; the reward itself is given inside a player's hook, where the
    /// game refuses one.
    pub fn flush(&self, host: &mut dyn Host) {
        let Ok(mut queue) = self.rewards.lock() else {
            return;
        };
        if queue.is_empty() {
            return;
        }
        let lines = std::mem::take(&mut *queue);
        drop(queue);
        for line in lines {
            if !host.run_command(&line) {
                eprintln!("pufferfish_skills: the game refused `{line}`");
            }
        }
    }

    /// Adds the node's attribute modifier to the player, permanently (as `/attribute ... modifier
    /// add` does: it is saved with them, so a death, a respawn or a rejoin keeps it).
    fn give_reward(&self, host: &mut dyn Host, uuid: u128, node: &str, reward: &Reward) {
        let Some(name) = Self::name_of(host, uuid) else {
            return;
        };
        self.queue(format!(
            "attribute {name} {} modifier add {}:{} {} {}",
            reward.attribute,
            MOD_ID,
            node,
            amount_text(reward.amount),
            reward.operation.command_name()
        ));
    }

    /// Takes the node's attribute modifier away again.
    fn take_reward(&self, host: &mut dyn Host, uuid: u128, node: &str, reward: &Reward) {
        let Some(name) = Self::name_of(host, uuid) else {
            return;
        };
        self.queue(format!(
            "attribute {name} {} modifier remove {}:{}",
            reward.attribute, MOD_ID, node
        ));
    }

    /// Queues every unlocked reward `uuid` has, and says how many. The modifiers `/attribute`
    /// adds are transient (the game does not save them with the player; its permanent-attribute
    /// helper is still an unnumbered hook point), so a login or a respawn gives them again from
    /// the progress the mod keeps itself.
    pub fn reapply(&self, host: &mut dyn Host, uuid: u128) -> usize {
        let Some(state) = self.state(uuid) else {
            return 0;
        };
        let mut applied = 0;
        for category in &crate::tree().categories {
            for node in state.unlocked_nodes(category) {
                if let Some(reward) = &node.reward {
                    self.give_reward(host, uuid, &node.id, reward);
                    applied += 1;
                }
            }
        }
        applied
    }

    /// Gives `amount` experience to `uuid`'s trees, saving and telling them what it earned.
    fn gain(&self, host: &mut dyn Host, uuid: u128, amount: i64, what: &str) {
        if amount <= 0 {
            return;
        }
        let gained = {
            let Ok(mut players) = self.players.lock() else {
                return;
            };
            let Some(state) = players.get_mut(&uuid) else {
                return;
            };
            state.add_xp(crate::tree(), amount)
        };
        self.save(host, uuid);
        let mut notice = None;
        for (category, points) in gained {
            let line = format!(
                "+{amount} experience ({what}): {points} skill point(s) in {category}"
            );
            notice = Some(line.clone());
            host.tell(uuid, Text::literal(line));
        }
        self.push(host, uuid, notice);
    }

    // MARK: hooks

    pub fn logged_in(&self, host: &mut dyn Host, uuid: u128, name: &str) {
        let state = match self.config(host) {
            Some(dir) => store::read(&dir, uuid),
            None => PlayerState::default(),
        };
        if let Ok(mut players) = self.players.lock() {
            players.insert(uuid, state);
        }
        let greeting = format!(
            "Skills: {name}. Press {} or type /skills to open your tree.",
            host.mod_key(MOD_ID, crate::KEY_NAME, crate::KEY_DEFAULT)
        );
        host.tell(uuid, Text::literal(greeting));
        self.push(host, uuid, None);
        self.reapply(host, uuid);
    }

    /// A player came back from death with a new body: give the rewards again, a tick late.
    pub fn respawned(&self, host: &mut dyn Host, uuid: u128) {
        self.reapply(host, uuid);
    }

    pub fn logged_out(&self, host: &mut dyn Host, uuid: u128) {
        if let Some(dir) = self.config(host)
            && let Ok(mut players) = self.players.lock()
            && let Some(state) = players.remove(&uuid)
        {
            store::write(&dir, uuid, &state);
        }
    }

    /// A message from the player's own client: their clicks in the tree's window.
    pub fn message(&self, host: &mut dyn Host, uuid: u128, channel: &str, data: &[u8]) {
        if channel != crate::CHANNEL_ACTION {
            return;
        }
        let Ok(text) = std::str::from_utf8(data) else {
            return;
        };
        let Some(request) = net::decode_request(text) else {
            return;
        };
        if self.state(uuid).is_none() {
            return;
        }
        match request {
            Request::Sync => self.push(host, uuid, None),
            Request::Unlock { category, node } => {
                let outcome = {
                    let Ok(mut players) = self.players.lock() else {
                        return;
                    };
                    let Some(state) = players.get_mut(&uuid) else {
                        return;
                    };
                    match state.unlock(crate::tree(), &category, &node) {
                        Ok(reward) => Ok(reward),
                        Err(refused) => Err(refused.message()),
                    }
                };
                match outcome {
                    Ok(reward) => {
                        if let Some(reward) = &reward {
                            self.give_reward(host, uuid, &node, reward);
                        }
                        self.save(host, uuid);
                        let label = crate::tree()
                            .node(&category, &node)
                            .map(|node| node.name.clone())
                            .unwrap_or_else(|| node.clone());
                        self.push(host, uuid, Some(format!("Unlocked {label}")));
                    }
                    Err(message) => self.push(host, uuid, Some(message)),
                }
            }
            Request::Reset { category } => {
                let removed = {
                    let Ok(mut players) = self.players.lock() else {
                        return;
                    };
                    let Some(state) = players.get_mut(&uuid) else {
                        return;
                    };
                    state.reset(crate::tree(), &category)
                };
                for node in &removed {
                    if let Some(reward) = crate::tree().node(&category, node).and_then(|n| n.reward.clone()) {
                        self.take_reward(host, uuid, node, &reward);
                    }
                }
                self.save(host, uuid);
                let count = removed.len();
                self.push(host, uuid, Some(format!("Reset: {count} skill(s) refunded")));
            }
        }
    }

    pub fn mob_killed(&self, host: &mut dyn Host, uuid: u128, kind: &str) {
        let xp = crate::tree().mob_xp_for(&skill_tree::namespaced(kind));
        self.gain(host, uuid, xp, kind);
    }

    pub fn block_broken(&self, host: &mut dyn Host, pos: [i32; 3]) {
        let block = host.block(pos);
        let xp = crate::tree().block_xp_for(&block);
        if xp <= 0 {
            return;
        }
        let Some(player) = host.acting_player() else {
            return;
        };
        self.gain(host, player.uuid, xp, &block);
    }

    /// A player completed an advancement. Recipe unlocks are worth nothing (see
    /// `advancement_muted` in `data/tree.json`): they are progress, not a deed.
    pub fn advancement_done(&self, host: &mut dyn Host, uuid: u128, advancement: &str) {
        let id = skill_tree::namespaced(advancement);
        let xp = crate::tree().advancement_xp_for(&id);
        self.gain(host, uuid, xp, &format!("advancement {id}"));
    }

    /// A player's fishing rod: state 2 is the reel-in of a bite, which gives the catch; 3 and 4
    /// reel nothing in, and 0 and 1 are the cast and the bite.
    pub fn fished(&self, host: &mut dyn Host, uuid: u128, state: i32) {
        if state != 2 {
            return;
        }
        let xp = crate::tree().fish_xp;
        self.gain(host, uuid, xp, "fishing");
    }

    /// A player took a crafting grid's result.
    pub fn crafted(&self, host: &mut dyn Host, uuid: u128, recipe: &str) {
        let id = skill_tree::namespaced(recipe);
        let xp = crate::tree().craft_xp_for(&id);
        self.gain(host, uuid, xp, &id);
    }

    /// A player bred two animals (the hook names the feeder, 0 for none).
    pub fn bred(&self, host: &mut dyn Host, uuid: u128, kind: &str) {
        let xp = crate::tree().breed_xp;
        let what = format!("breeding {}", skill_tree::namespaced(kind));
        self.gain(host, uuid, xp, &what);
    }

    /// The commands a server's players get: `/skillsadmin`, for whoever runs the server.
    pub fn register(&self, commands: &mut Vec<NativeCommand>) {
        for usage in [
            "skillsadmin points add <player:player> <category:word> <amount:integer>",
            "skillsadmin points set <player:player> <category:word> <amount:integer>",
            "skillsadmin unlock <player:player> <category:word> <skill:word>",
            "skillsadmin reset <player:player> <category:word>",
            "skillsadmin sync <player:player>",
            "skillsadmin info [player:player]",
        ] {
            commands.push(NativeCommand::op(usage, 2));
        }
    }

    /// A player's `/skillsadmin` line, which this mod answers or leaves alone.
    pub fn command(&self, host: &mut dyn Host, player: &NativeServerPlayer, line: &str) -> bool {
        let words: Vec<&str> = line.split_whitespace().collect();
        if words.first() != Some(&"skillsadmin") {
            return false;
        }
        let reply = self.run(host, Some(player), &words);
        host.feedback(Text::literal(reply.unwrap_or_else(usage_text)));
        true
    }

    /// The same line typed on the dedicated server's console, where there is no player: `info`
    /// needs one, the rest name the player they act on.
    pub fn console_command(&self, host: &mut dyn Host, line: &str) -> bool {
        let words: Vec<&str> = line.split_whitespace().collect();
        if words.first() != Some(&"skillsadmin") {
            return false;
        }
        let reply = self.run(host, None, &words);
        host.feedback(Text::literal(reply.unwrap_or_else(usage_text)));
        true
    }

    /// What a well-formed line did, or why it did nothing.
    fn run(
        &self,
        host: &mut dyn Host,
        player: Option<&NativeServerPlayer>,
        words: &[&str],
    ) -> Option<String> {
        let players = host.server_players();
        let find = |name: &str| players.iter().find(|p| p.name == name).map(|p| p.uuid);
        match words {
            ["skillsadmin", "points", how, name, category, amount] => {
                let uuid = find(*name)?;
                let category_ref = crate::tree().category(category)?;
                let amount: i64 = amount.parse().ok()?;
                if amount < 0 {
                    return Some("a negative amount changes nothing".to_owned());
                }
                {
                    let mut players = self.players.lock().ok()?;
                    let state = players.entry(uuid).or_default();
                    let progress = state.categories.entry(category_ref.id.clone()).or_default();
                    match *how {
                        "add" => {
                            progress.xp = progress
                                .xp
                                .saturating_add(amount.saturating_mul(category_ref.xp_per_point));
                        }
                        "set" => {
                            progress.xp = progress
                                .spent
                                .saturating_add(amount)
                                .saturating_mul(category_ref.xp_per_point);
                        }
                        _ => return Some(usage_text()),
                    }
                }
                self.save(host, uuid);
                self.push(host, uuid, None);
                Some(format!("{name}: {amount} point(s) in {category}"))
            }
            ["skillsadmin", "unlock", name, category, skill] => {
                let uuid = find(*name)?;
                let reward = crate::tree().node(category, skill)?.reward.clone();
                let cost = crate::tree().node(category, skill)?.cost;
                {
                    let mut players = self.players.lock().ok()?;
                    let state = players.entry(uuid).or_default();
                    state.unlocked.insert(state::key(category, skill));
                    state
                        .categories
                        .entry(category.to_string())
                        .or_default()
                        .spent += cost;
                }
                if let Some(reward) = &reward {
                    self.give_reward(host, uuid, skill, reward);
                }
                self.save(host, uuid);
                self.push(host, uuid, Some(format!("Unlocked {category}/{skill}")));
                Some(format!("{name}: unlocked {category}/{skill}"))
            }
            ["skillsadmin", "reset", name, category] => {
                let uuid = find(*name)?;
                let removed = {
                    let mut players = self.players.lock().ok()?;
                    let state = players.entry(uuid).or_default();
                    state.reset(crate::tree(), category)
                };
                for node in &removed {
                    if let Some(reward) = crate::tree().node(category, node).and_then(|n| n.reward.clone()) {
                        self.take_reward(host, uuid, node, &reward);
                    }
                }
                self.save(host, uuid);
                self.push(host, uuid, Some(format!("Reset: {} skill(s)", removed.len())));
                Some(format!("{name}: {} skill(s) reset", removed.len()))
            }
            ["skillsadmin", "sync", name] => {
                let uuid = find(*name)?;
                let applied = self.reapply(host, uuid);
                Some(format!("{name}: {applied} reward(s) re-applied"))
            }
            ["skillsadmin", "info"] => player.map(|player| self.describe(player.uuid)),
            ["skillsadmin", "info", name] => {
                let uuid = find(*name)?;
                Some(self.describe(uuid))
            }
            _ => None,
        }
    }

    fn describe(&self, uuid: u128) -> String {
        let Some(state) = self.state(uuid) else {
            return "no progress".to_owned();
        };
        let categories = state
            .categories
            .iter()
            .map(|(id, progress)| format!("{id}: {} xp, {} spent", progress.xp, progress.spent))
            .collect::<Vec<_>>()
            .join("; ");
        format!("{} skill(s) unlocked ({categories})", state.unlocked.len())
    }
}

fn usage_text() -> String {
    "usage: /skillsadmin points add|set <player> <category> <amount>; unlock <player> <category> \
     <skill>; reset <player> <category>; sync <player>; info [player]"
        .to_owned()
}

/// `2.0` written as `2`, `0.15` as `0.15`: what the attribute command parses.
fn amount_text(amount: f64) -> String {
    let text = format!("{amount}");
    if text.contains("e") || text.contains("E") {
        format!("{amount:.6}")
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A server that keeps the command lines it is given, as the game's console would run them.
    #[derive(Default)]
    struct Recorder {
        lines: Mutex<Vec<String>>,
    }

    impl Host for Recorder {
        fn run_command(&mut self, line: &str) -> bool {
            self.lines.lock().unwrap().push(line.to_owned());
            true
        }
    }

    #[test]
    fn a_queued_reward_runs_at_the_next_tick_once() {
        let server = Server::default();
        let mut host = Recorder::default();
        server.queue(
            "attribute Alex minecraft:max_health modifier add pufferfish_skills:vitality_1 2 \
             add_value"
                .to_owned(),
        );
        server.queue(
            "attribute Alex minecraft:armor modifier add pufferfish_skills:toughness 1 add_value"
                .to_owned(),
        );
        server.flush(&mut host);
        server.flush(&mut host);
        let lines = host.lines.lock().unwrap();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("attribute Alex minecraft:max_health modifier add"));
        assert!(lines[1].starts_with("attribute Alex minecraft:armor modifier add"));
    }

    #[test]
    fn amounts_are_plain_numbers() {
        assert_eq!(amount_text(2.0), "2");
        assert_eq!(amount_text(0.15), "0.15");
        assert_eq!(amount_text(-1.0), "-1");
    }
}
