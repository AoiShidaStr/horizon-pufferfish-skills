//! The skill tree: categories, nodes and their attribute rewards.
//!
//! Read from `data/tree.json`, which is compiled into the mod (`include_str!`), so both halves of
//! the mod (the window's and the server's) read the same tree without a content mod or a reload.

use horizon_client::mod_api::serde_json::{self, Value};

/// The tree this mod ships with.
pub const DEFAULT_TREE: &str = include_str!("../data/tree.json");

/// How a reward's amount applies to its attribute: vanilla's `AttributeModifier.Operation`, as
/// `/attribute ... modifier add` spells it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    AddValue,
    AddMultipliedBase,
    AddMultipliedTotal,
}

impl Operation {
    fn parse(text: &str) -> Option<Self> {
        match text {
            "add_value" => Some(Self::AddValue),
            "add_multiplied_base" => Some(Self::AddMultipliedBase),
            "add_multiplied_total" => Some(Self::AddMultipliedTotal),
            _ => None,
        }
    }

    /// The name the attribute command takes.
    pub fn command_name(self) -> &'static str {
        match self {
            Self::AddValue => "add_value",
            Self::AddMultipliedBase => "add_multiplied_base",
            Self::AddMultipliedTotal => "add_multiplied_total",
        }
    }
}

/// What unlocking a node gives: a permanent attribute modifier.
#[derive(Clone, Debug, PartialEq)]
pub struct Reward {
    /// `minecraft:max_health`.
    pub attribute: String,
    pub amount: f64,
    pub operation: Operation,
}

/// One box of the tree.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: String,
    pub name: String,
    pub description: String,
    /// The item drawn in the box (`minecraft:iron_sword`).
    pub icon: String,
    /// Points the node costs.
    pub cost: i64,
    /// The node's place in the tree's grid.
    pub x: i32,
    pub y: i32,
    /// The nodes that must be unlocked first.
    pub parents: Vec<String>,
    pub reward: Option<Reward>,
}

/// A tree of its own: its own points and its own nodes.
#[derive(Clone, Debug, PartialEq)]
pub struct Category {
    pub id: String,
    pub name: String,
    pub icon: String,
    /// How much experience one point of this category costs.
    pub xp_per_point: i64,
    pub nodes: Vec<Node>,
}

/// The whole tree: its categories and what the game's own deeds are worth.
#[derive(Clone, Debug, PartialEq)]
pub struct Tree {
    pub categories: Vec<Category>,
    /// Experience per mob kind, `default` for the rest.
    pub mob_xp: Vec<(String, i64)>,
    /// Experience per broken block, `default` for the rest.
    pub block_xp: Vec<(String, i64)>,
    /// Experience per completed advancement, `default` for the rest.
    pub advancement_xp: Vec<(String, i64)>,
    /// Advancement id prefixes worth nothing: the recipe unlocks are progress, not a deed.
    pub advancement_muted: Vec<String>,
    /// Experience per crafted recipe, `default` for the rest.
    pub craft_xp: Vec<(String, i64)>,
    /// Experience for a fish reeled in with a catch.
    pub fish_xp: i64,
    /// Experience for a baby bred by the player.
    pub breed_xp: i64,
}

impl Tree {
    /// A tree with nothing in it, for a `data/tree.json` that does not parse.
    pub fn empty() -> Self {
        Self {
            categories: Vec::new(),
            mob_xp: Vec::new(),
            block_xp: Vec::new(),
            advancement_xp: Vec::new(),
            advancement_muted: Vec::new(),
            craft_xp: Vec::new(),
            fish_xp: 0,
            breed_xp: 0,
        }
    }

    pub fn category(&self, id: &str) -> Option<&Category> {
        self.categories.iter().find(|c| c.id == id)
    }

    pub fn category_index(&self, id: &str) -> Option<usize> {
        self.categories.iter().position(|c| c.id == id)
    }

    pub fn node(&self, category: &str, id: &str) -> Option<&Node> {
        self.category(category)?.nodes.iter().find(|n| n.id == id)
    }

    /// What killing `kind` is worth.
    pub fn mob_xp_for(&self, kind: &str) -> i64 {
        lookup(&self.mob_xp, kind)
    }

    /// What breaking `block` is worth.
    pub fn block_xp_for(&self, block: &str) -> i64 {
        lookup(&self.block_xp, block)
    }

    /// What completing `advancement` is worth; nothing at all for a muted prefix.
    pub fn advancement_xp_for(&self, advancement: &str) -> i64 {
        if self
            .advancement_muted
            .iter()
            .any(|prefix| advancement.starts_with(prefix.as_str()))
        {
            return 0;
        }
        lookup(&self.advancement_xp, advancement)
    }

    /// What crafting `recipe` is worth.
    pub fn craft_xp_for(&self, recipe: &str) -> i64 {
        lookup(&self.craft_xp, recipe)
    }
}

fn lookup(table: &[(String, i64)], id: &str) -> i64 {
    table
        .iter()
        .find(|(name, _)| name == id)
        .or_else(|| table.iter().find(|(name, _)| name == "default"))
        .map_or(0, |(_, xp)| *xp)
}

/// `minecraft:` in front of an id without a namespace.
pub fn namespaced(id: &str) -> String {
    if id.contains(':') {
        id.to_owned()
    } else {
        format!("minecraft:{id}")
    }
}

/// Whether `text` is a mod id in the shape the tree and the game take.
pub fn plain_id(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 64
        && text
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// One amount, 0 when it is not there.
fn number(value: Option<&Value>, what: &str) -> Result<i64, String> {
    let Some(value) = value else {
        return Ok(0);
    };
    let xp = value
        .as_i64()
        .ok_or_else(|| format!("{what} is not a whole number"))?;
    if xp < 0 {
        return Err(format!("{what} is negative"));
    }
    Ok(xp)
}

/// A list of id prefixes, each a name like `minecraft:recipes/`.
fn prefixes(value: Option<&Value>) -> Result<Vec<String>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let Value::Array(list) = value else {
        return Err("advancement_muted is not a list".to_owned());
    };
    let mut out = Vec::new();
    for entry in list {
        let prefix = entry
            .as_str()
            .filter(|text| !text.is_empty())
            .ok_or("advancement_muted has an entry that is not a name")?;
        out.push(prefix.to_owned());
    }
    Ok(out)
}

/// Reads a tree. A malformed tree is refused whole, with what is wrong named: a node nobody can
/// reach or a reward the attribute command would not take is a typo, not a node to skip.
pub fn parse(text: &str) -> Result<Tree, String> {
    let root: Value = serde_json::from_str(text).map_err(|e| format!("not JSON: {e}"))?;
    let mob_xp = table(root.get("mob_xp"), "mob_xp")?;
    let block_xp = table(root.get("block_xp"), "block_xp")?;
    let advancement_xp = table(root.get("advancement_xp"), "advancement_xp")?;
    let advancement_muted = prefixes(root.get("advancement_muted"))?;
    let craft_xp = table(root.get("craft_xp"), "craft_xp")?;
    let fish_xp = number(root.get("fish_xp"), "fish_xp")?;
    let breed_xp = number(root.get("breed_xp"), "breed_xp")?;
    let categories = root
        .get("categories")
        .and_then(Value::as_array)
        .ok_or("no categories")?;
    if categories.is_empty() {
        return Err("no categories".into());
    }
    let mut out = Vec::new();
    for value in categories {
        let category = category(value)?;
        if out.iter().any(|c: &Category| c.id == category.id) {
            return Err(format!("two categories named {}", category.id));
        }
        out.push(category);
    }
    Ok(Tree {
        categories: out,
        mob_xp,
        block_xp,
        advancement_xp,
        advancement_muted,
        craft_xp,
        fish_xp,
        breed_xp,
    })
}

/// A table of amounts: a missing table is no source at all, and every row is a whole number.
fn table(value: Option<&Value>, what: &str) -> Result<Vec<(String, i64)>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let Value::Object(map) = value else {
        return Err(format!("{what} is not an object"));
    };
    let mut out = Vec::new();
    for (key, value) in map {
        let xp = value
            .as_i64()
            .ok_or_else(|| format!("{what}.{key} is not a whole number"))?;
        if xp < 0 {
            return Err(format!("{what}.{key} is negative"));
        }
        // `minecraft:default` is the fallback row, as a mob kind would be namespaced; `lookup`
        // asks for `default`.
        let key = if key == "minecraft:default" {
            "default".to_owned()
        } else {
            key.clone()
        };
        out.push((key, xp));
    }
    Ok(out)
}

fn category(value: &Value) -> Result<Category, String> {
    let id = text(value, "id", "category")?;
    if !plain_id(&id) {
        return Err(format!("category id {id:?} is not [a-z0-9_]{{1,64}}"));
    }
    let xp_per_point = integer(value.get("xp_per_point"), &id, "xp_per_point")?;
    if xp_per_point < 1 {
        return Err(format!("{id}: xp_per_point must be 1 or more"));
    }
    let nodes = value
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{id}: no nodes"))?;
    if nodes.is_empty() {
        return Err(format!("{id}: no nodes"));
    }
    let mut out = Vec::new();
    for value in nodes {
        let node = node(value, &id)?;
        if out.iter().any(|n: &Node| n.id == node.id) {
            return Err(format!("{id}: two nodes named {}", node.id));
        }
        out.push(node);
    }
    for node in &out {
        for parent in &node.parents {
            if !out.iter().any(|n| &n.id == parent) {
                return Err(format!("{id}/{}: no parent {parent}", node.id));
            }
        }
    }
    Ok(Category {
        id,
        name: text(value, "name", "category")?,
        icon: namespaced(&text(value, "icon", "category")?),
        xp_per_point,
        nodes: out,
    })
}

fn node(value: &Value, category: &str) -> Result<Node, String> {
    let id = text(value, "id", category)?;
    if !plain_id(&id) {
        return Err(format!("{category}: node id {id:?} is not [a-z0-9_]{{1,64}}"));
    }
    let cost = integer(value.get("cost"), &id, "cost")?;
    if cost < 1 {
        return Err(format!("{category}/{id}: cost must be 1 or more"));
    }
    let mut parents = Vec::new();
    match value.get("parents") {
        None => {}
        Some(Value::Array(list)) => {
            for parent in list {
                let parent = parent
                    .as_str()
                    .ok_or_else(|| format!("{category}/{id}: a parent is not a name"))?;
                parents.push(parent.to_owned());
            }
        }
        Some(_) => return Err(format!("{category}/{id}: parents is not a list")),
    }
    if parents.iter().any(|parent| parent == &id) {
        return Err(format!("{category}/{id}: it is its own parent"));
    }
    let reward = match value.get("reward") {
        None | Some(Value::Null) => None,
        Some(reward) => Some(Reward {
            attribute: namespaced(
                reward
                    .get("attribute")
                    .and_then(Value::as_str)
                    .ok_or_else(|| format!("{category}/{id}: reward has no attribute"))?,
            ),
            amount: reward
                .get("amount")
                .and_then(Value::as_f64)
                .ok_or_else(|| format!("{category}/{id}: reward has no amount"))?,
            operation: reward
                .get("operation")
                .and_then(Value::as_str)
                .and_then(Operation::parse)
                .ok_or_else(|| format!("{category}/{id}: reward has no known operation"))?,
        }),
    };
    Ok(Node {
        id,
        name: text(value, "name", category)?,
        description: value
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        icon: namespaced(&text(value, "icon", category)?),
        cost,
        x: value.get("x").and_then(Value::as_i64).unwrap_or(0) as i32,
        y: value.get("y").and_then(Value::as_i64).unwrap_or(0) as i32,
        parents,
        reward,
    })
}

fn text(value: &Value, key: &str, what: &str) -> Result<String, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("{what}: no {key}"))
}

fn integer(value: Option<&Value>, what: &str, key: &str) -> Result<i64, String> {
    value
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("{what}: {key} is not a whole number"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_tree_parses() {
        let tree = parse(DEFAULT_TREE).expect("the shipped tree parses");
        assert!(tree.categories.len() >= 2);
        assert!(tree.category("adventure").is_some());
        let node = tree.node("adventure", "vitality_1").expect("a node");
        assert_eq!(node.cost, 1);
        assert!(node.reward.is_some());
        assert_eq!(tree.mob_xp_for("minecraft:zombie"), 6);
        assert_eq!(tree.mob_xp_for("minecraft:no_such_mob"), 5);
        assert_eq!(tree.block_xp_for("minecraft:diamond_ore"), 12);
        assert_eq!(tree.block_xp_for("minecraft:dirt"), 0);
    }

    #[test]
    fn every_source_has_its_value() {
        let tree = parse(DEFAULT_TREE).expect("the shipped tree parses");
        assert_eq!(tree.advancement_xp_for("minecraft:story/mine_stone"), 25);
        assert_eq!(tree.advancement_xp_for("minecraft:story/no_such_deed"), 10);
        // Recipe unlocks are progress, not a deed.
        assert_eq!(tree.advancement_xp_for("minecraft:recipes/building_blocks/oak_planks"), 0);
        assert_eq!(tree.craft_xp_for("minecraft:diamond_pickaxe"), 6);
        assert_eq!(tree.craft_xp_for("minecraft:stick"), 1);
        assert!(tree.fish_xp > 0);
        assert!(tree.breed_xp > 0);
    }

    #[test]
    fn a_missing_source_is_worth_nothing() {
        let bare = r#"{"categories":[
            {"id":"a","name":"A","icon":"minecraft:stone","xp_per_point":10,
             "nodes":[{"id":"one","name":"One","icon":"minecraft:stone","cost":1}]}]}"#;
        let tree = parse(bare).expect("a tree with no tables parses");
        assert_eq!(tree.advancement_xp_for("minecraft:story/mine_stone"), 0);
        assert_eq!(tree.craft_xp_for("minecraft:stick"), 0);
        assert_eq!(tree.fish_xp, 0);
        assert_eq!(tree.breed_xp, 0);
    }

    #[test]
    fn every_parent_exists() {
        let tree = parse(DEFAULT_TREE).unwrap();
        for category in &tree.categories {
            for node in &category.nodes {
                for parent in &node.parents {
                    assert!(
                        category.nodes.iter().any(|n| &n.id == parent),
                        "{} has no parent {parent}",
                        node.id
                    );
                }
            }
        }
    }

    #[test]
    fn a_malformed_tree_is_refused() {
        assert!(parse("{}").is_err());
        assert!(parse("not json").is_err());
        let no_parent = r#"{"mob_xp":{},"block_xp":{},"categories":[
            {"id":"a","name":"A","icon":"minecraft:stone","xp_per_point":10,
             "nodes":[{"id":"one","name":"One","icon":"minecraft:stone","cost":1,
                       "parents":["gone"]}]}]}"#;
        let error = parse(no_parent).unwrap_err();
        assert!(error.contains("gone"), "{error}");
        let bad_op = r#"{"mob_xp":{},"block_xp":{},"categories":[
            {"id":"a","name":"A","icon":"minecraft:stone","xp_per_point":10,
             "nodes":[{"id":"one","name":"One","icon":"minecraft:stone","cost":1,
                       "reward":{"attribute":"max_health","amount":1,"operation":"nope"}}]}]}"#;
        assert!(parse(bad_op).is_err());
        let bad_kind = r#"{"fish_xp":"lots","categories":[
            {"id":"a","name":"A","icon":"minecraft:stone","xp_per_point":10,
             "nodes":[{"id":"one","name":"One","icon":"minecraft:stone","cost":1}]}]}"#;
        assert!(parse(bad_kind).is_err());
        let free = r#"{"mob_xp":{},"block_xp":{},"categories":[
            {"id":"a","name":"A","icon":"minecraft:stone","xp_per_point":10,
             "nodes":[{"id":"one","name":"One","icon":"minecraft:stone","cost":0}]}]}"#;
        assert!(parse(free).is_err());
    }

    #[test]
    fn ids_are_namespaced_once() {
        assert_eq!(namespaced("max_health"), "minecraft:max_health");
        assert_eq!(namespaced("minecraft:max_health"), "minecraft:max_health");
        assert_eq!(namespaced("mymod:thing"), "mymod:thing");
    }
}
