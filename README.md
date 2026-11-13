# Pufferfish's Skills — Horizon port

A permanent skill tree for the Saison 2 server: experience earned in play becomes points, points
buy boxes of a tree, and every box grants a permanent attribute reward. The tree opens in a window
of its own (`K` by default, rebindable in Options > Mods, or `/skills`).

- Mod id: `pufferfish_skills`
- Kind: **native mod** (`.hnmod`, a trait mod — no content mod, no mixin)
- Side: **both** — one bundle, the server half and the client half of the same package
- Built against Horizon commit `a85af177a82504f357ff10d5d40d0b7230e2ac32` (hook points 415)

## Why a clean-room rewrite

Upstream *Pufferfish's Skills* (CurseForge project 835091 / Modrinth `skills`) ships under a
custom licence: "You can use this mod in your modpack as long as you do not redistribute this
mod." That is not a licence to copy or translate its code, data or textures, so **none of it is
used here**. Everything below — the tree, the points, the rewards, the window, the protocol — is
original code written from the public description of what such a mod does (a tree of nodes, XP
turned into points, attribute rewards). No texture of the original is used either: the window is
drawn from filled rectangles, the game's text and vanilla item icons.

There was no Horizon port of Pufferfish's Skills in the registry index when this was written
(checked against `https://horizon-rust.github.io/horizon-mods/index.json`, 166 projects).

This port's own code is MIT (see `LICENSE`). "Pufferfish's Skills" is the name of the original mod
by Pufferfish; this is an unaffiliated reimplementation of the idea for a private server.

## Done

- **Skill tree** (`data/tree.json`, compiled into the mod): categories, nodes with a cost, a grid
  position, parents and an attribute reward. Two categories ship, `adventure` (7 nodes) and
  `gathering` (6 nodes); the whole tree is validated when it is first read (unknown parents,
  duplicate ids, a cost of 0 or an unknown modifier operation refuse it whole).
- **Experience**, five sources, all from the tables in `data/tree.json`: killing a mob
  (`mob_xp`, `minecraft:zombie` 6), breaking a block (`block_xp`, `minecraft:diamond_ore` 12),
  completing an advancement (`advancement_xp`, `minecraft:story/mine_stone` 25,
  `minecraft:end/kill_dragon` 300, 10 by default; the `advancement_muted` prefixes —
  `minecraft:recipes/` — earn nothing, a recipe unlock is progress and not a deed), taking a
  crafted recipe (`craft_xp`, 1 by default, a diamond tool or armour 6-7), the reel-in of a
  bite (`fish_xp`, 8) and a baby bred by the player (`breed_xp`, 10). Every table has a
  `default` row or is one flat number, so a server can retune a whole source in one edit. Every
  category earns the same experience and turns it into points at its own rate (`xp_per_point`:
  120 for `adventure`, 100 for `gathering`).
- **Points and unlocks**, on the server: a node needs its parents unlocked and enough unspent
  points; unlocking spends the cost and gives the reward; a category reset refunds every point and
  takes the rewards back.
- **Rewards** are permanent attribute modifiers (`/attribute <player> <attribute> modifier add`),
  so they are saved with the player and survive death, respawn and rejoin: `max_health`, `armor`,
  `movement_speed`, `attack_damage`, `block_break_speed`, `luck` in the shipped tree.
- **The window**: an in-game screen of the mod's own (Horizon's `native_mod_screen_open`), drawn
  with the screen hook — panel, category tabs, the tree's links, one box a node with its item
  icon, the hovered skill's name/description/cost, the points left, the server's last message.
  Click a box to unlock, a tab to switch category, the `x` to close; Escape closes it too.
- **Input**: a keybind (default `key.keyboard.k`) listed in Options > Mods, and the client command
  `/skills`, which opens the tree from anywhere.
- **Persistence**: `config/pufferfish_skills/players/<uuid>.json` on the server (written on every
  change, on logout, and read on login), one file a player.
- **Server commands** (`/skillsadmin`, operator level 2): `points add|set <player> <category>
  <amount>`, `unlock <player> <category> <skill>`, `reset <player> <category>`, `sync <player>`
  (re-applies the rewards of what is unlocked — for a player whose saved modifiers were lost),
  `info [player]`.
- **Client/server split**: the client only asks (`horizon:mod_net`, channel
  `pufferfish_skills:action`), the server checks every request against its own tree and its own
  copy of the player's progress, and answers with a snapshot
  (`pufferfish_skills:state`).

## Missing / not done

- **No automated in-game test.** The logic, the tree, the storage format and the message format
  are unit-tested (`cargo test`), and the whole mod compiles against the real game sources, but
  nobody has played a session with it: the window has not been seen on screen, and the attribute
  command path has not been observed on a live server. This is the main gap.
- **No XP for smelting or play time**: Horizon's `furnace_smelt` hook names the furnace and not
  the player who loaded it, and there is no per-minute hook (`player_tick` runs 20 times a
  second and would need a timer of the mod's own). Everything else a player does with their
  hands — kill, mine, advance, craft, fish, breed — earns.
- **No cap on a farmed source**: a player who crafts 1 000 cheap items earns 1 000 experience.
  Crafted experience is per take of the grid, so a full stack counts once, but an item farm is
  still worth more than a fight. If that matters, tune `craft_xp`'s `default` down.
- **The tree is not editable at runtime**: it lives in the bundle (`api/data/tree.json`) and needs
  a rebuild of the mod. Pufferfish's Skills reads its trees from a data pack and has an online
  editor; this has neither.
- **No `neoforge:conditions`-style gating, no per-node effects other than attribute modifiers**
  (no commands on unlock, no item or effect rewards, no repeatable nodes).
- **No texture**: the window is deliberately drawn from rectangles and vanillas icons; it is
  functional, not pretty.
- **Attributes are applied through the console command** because Horizon's direct per-player
  attribute helpers (`@API:player_attributes@`, `@API:player_persistent_data@`) are still pending
  hook points. When they are numbered, the mod should use them and drop the command round-trip
  (and then a player's skill data can live in their player file instead of `config/`).
- **No registry publication**: not signed, not released, not listed (this is a private server's
  port).

## Files

```
mods/pufferfish_skills/
  horizon-native.json               native manifest packed at the .hnmod's root
  mixins/pufferfish_skills/
    horizon-mixin.toml              [mod] + [compatibility] + [api] path
    api/Cargo.toml                  the trait-mod package (depends on horizon-client only)
    api/data/tree.json              the shipped skill tree + XP tables
    api/src/lib.rs                  Hooks: the wiring
    api/src/server.rs               experience (five sources), points, unlocks, rewards, commands
    api/src/client.rs               key, /skills, the window, its clicks, the server's state
    api/src/gui.rs                  the window's layout and drawing
    api/src/net.rs                  the two halves' messages
    api/src/state.rs                a player's progress and the rules
    api/src/skill_tree.rs           the tree model and its parser
    api/src/store.rs                config/pufferfish_skills/players/<uuid>.json
  tools/pack.py                     packs dist/pufferfish_skills-<version>.hnmod
  dist/                             build artefacts
```

## Building and checking (VPS)

The bundle is a trait mod, so it is built into a game the way the launcher builds one: the
`api/` package is staged as a workspace member beside `horizon-client`, and a generated
`crates/native-game` binary links `hooks()`. On the build host:

```sh
cd /opt/build/horizon && git worktree add --detach /opt/build/wt/pufferfish_skills
scp -r mods/pufferfish_skills root@host:/opt/build/mods/
/opt/build/ps-stage.sh                     # stages api/ + crates/native-game
export CARGO_TARGET_DIR=/opt/build/target CARGO_BUILD_JOBS=4
cargo check -p mod-pufferfish_skills        # the mod against the real game sources
cargo test  -p mod-pufferfish_skills        # the unit tests
cargo build -p horizon-native-game --no-default-features   # the dedicated-server engine
cargo check -p horizon-native-game          # the client flavour
```

`python tools/pack.py` writes `dist/pufferfish_skills-0.1.0.hnmod` (a zip of
`horizon-native.json` and `mixins/pufferfish_skills/`), which is what a player or a server
installs (`horizon-native-mods/`), built by the launcher on install.

## Installing

- **Server**: put `pufferfish_skills-0.1.0.hnmod` in `horizon-native-mods/`, add
  `mod.pufferfish_skills:true` to `horizon-mods.txt`, and let the launcher build the server engine
  with it (`--server-output`). The player files land in `config/pufferfish_skills/`.
- **Client**: the same `.hnmod` in `horizon-native-mods/`, the same `mod.pufferfish_skills:true`;
  the launcher builds the game with it.

Both halves come from the same bundle, so a server and its players must run the same version.

## Licence notices

- This port: MIT (`LICENSE`). Original code, no upstream file copied.
- Upstream Pufferfish's Skills: custom licence, **not** redistributable — nothing from it is
  included. Its name is used only to say what this reimplements.
- Horizon and horizon-mixins: see their own repositories (Horizon is licensed by its authors); this
  mod only links against `horizon-client`'s public mod API.
