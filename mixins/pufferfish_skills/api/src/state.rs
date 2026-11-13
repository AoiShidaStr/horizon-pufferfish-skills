//! A player's progress: the experience of each category, the points spent in it, and the nodes
//! unlocked. The rules live here, away from the game, so they are tested on their own.

use std::collections::{BTreeMap, BTreeSet};

use crate::skill_tree::{Category, Node, Reward, Tree};

/// The experience and the points spent in one category.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    pub xp: i64,
    /// Points spent unlocking nodes.
    pub spent: i64,
}

/// Everything the mod keeps of one player. It is saved in `config/pufferfish_skills/players/`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlayerState {
    pub categories: BTreeMap<String, Progress>,
    /// Unlocked nodes, as `category/node`.
    pub unlocked: BTreeSet<String>,
}

/// Why a node can't be unlocked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    NoCategory(String),
    NoNode(String),
    AlreadyUnlocked,
    Needs(String),
    Points { needed: i64, available: i64 },
}

impl Refused {
    /// What the player is told.
    pub fn message(&self) -> String {
        match self {
            Self::NoCategory(id) => format!("no category {id}"),
            Self::NoNode(id) => format!("no skill {id}"),
            Self::AlreadyUnlocked => "that skill is already unlocked".to_owned(),
            Self::Needs(parent) => format!("unlock {parent} first"),
            Self::Points { needed, available } => {
                format!("that skill costs {needed} points, you have {available}")
            }
        }
    }
}

/// The key a node is unlocked under.
pub fn key(category: &str, node: &str) -> String {
    format!("{category}/{node}")
}

impl PlayerState {
    /// The category's experience and spent points; nothing at all for a category not earned in.
    pub fn progress(&self, category: &str) -> Progress {
        self.categories.get(category).copied().unwrap_or_default()
    }

    /// The points of `category` the player may still spend.
    pub fn available(&self, category: &Category) -> i64 {
        let progress = self.progress(&category.id);
        let earned = progress.xp / category.xp_per_point.max(1);
        (earned - progress.spent).max(0)
    }

    pub fn is_unlocked(&self, category: &str, node: &str) -> bool {
        self.unlocked.contains(&key(category, node))
    }

    /// Every node of `category` unlocked, in the tree's order.
    pub fn unlocked_nodes<'a>(&'a self, category: &'a Category) -> impl Iterator<Item = &'a Node> {
        category
            .nodes
            .iter()
            .filter(move |node| self.is_unlocked(&category.id, &node.id))
    }

    /// Whether `node` may be unlocked now, and why not when it may not.
    pub fn can_unlock(&self, tree: &Tree, category: &str, node: &str) -> Result<(), Refused> {
        let category_ref = tree
            .category(category)
            .ok_or_else(|| Refused::NoCategory(category.to_owned()))?;
        let node_ref = category_ref
            .nodes
            .iter()
            .find(|n| n.id == node)
            .ok_or_else(|| Refused::NoNode(node.to_owned()))?;
        if self.is_unlocked(category, node) {
            return Err(Refused::AlreadyUnlocked);
        }
        for parent in &node_ref.parents {
            if !self.is_unlocked(category, parent) {
                return Err(Refused::Needs(parent.clone()));
            }
        }
        let available = self.available(category_ref);
        if available < node_ref.cost {
            return Err(Refused::Points {
                needed: node_ref.cost,
                available,
            });
        }
        Ok(())
    }

    /// Unlocks `node`, spending its cost, and gives back what it rewards.
    pub fn unlock(
        &mut self,
        tree: &Tree,
        category: &str,
        node: &str,
    ) -> Result<Option<Reward>, Refused> {
        self.can_unlock(tree, category, node)?;
        let node_ref = tree
            .node(category, node)
            .ok_or_else(|| Refused::NoNode(node.to_owned()))?;
        let progress = self.categories.entry(category.to_owned()).or_default();
        progress.spent += node_ref.cost;
        self.unlocked.insert(key(category, node));
        Ok(node_ref.reward.clone())
    }

    /// Locks every node of `category` again, refunding the points; gives back the nodes whose
    /// rewards must be taken away.
    pub fn reset(&mut self, tree: &Tree, category: &str) -> Vec<String> {
        let Some(category_ref) = tree.category(category) else {
            return Vec::new();
        };
        let mut removed = Vec::new();
        for node in &category_ref.nodes {
            if self.unlocked.remove(&key(&category_ref.id, &node.id)) {
                if node.reward.is_some() {
                    removed.push(node.id.clone());
                }
            }
        }
        if let Some(progress) = self.categories.get_mut(&category_ref.id) {
            progress.spent = 0;
        }
        removed
    }

    /// Adds experience to every category, as vanilla experience is the player's and not a tree's.
    /// Gives back the categories whose points the player now has one more of.
    pub fn add_xp(&mut self, tree: &Tree, amount: i64) -> Vec<(String, i64)> {
        if amount <= 0 {
            return Vec::new();
        }
        let mut gained = Vec::new();
        for category in &tree.categories {
            let before = self.available(category);
            let progress = self.categories.entry(category.id.clone()).or_default();
            progress.xp = progress.xp.saturating_add(amount);
            let after = self.available(category);
            if after > before {
                gained.push((category.id.clone(), after - before));
            }
        }
        gained
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skill_tree as tree;

    fn tree() -> Tree {
        tree::parse(tree::DEFAULT_TREE).unwrap()
    }

    #[test]
    fn points_come_from_experience() {
        let tree = tree();
        let mut state = PlayerState::default();
        let adventure = tree.category("adventure").unwrap();
        assert_eq!(state.available(adventure), 0);
        // One point every 120 experience.
        state.add_xp(&tree, 119);
        assert_eq!(state.available(adventure), 0);
        state.add_xp(&tree, 1);
        assert_eq!(state.available(adventure), 1);
        // Gathering costs less: one point every 100.
        let gathering = tree.category("gathering").unwrap();
        assert_eq!(state.available(gathering), 1);
        state.add_xp(&tree, 100);
        assert_eq!(state.available(adventure), 1);
        assert_eq!(state.available(gathering), 2);
    }

    #[test]
    fn a_point_is_told_once() {
        let tree = tree();
        let mut state = PlayerState::default();
        assert!(state.add_xp(&tree, 50).is_empty());
        let gained = state.add_xp(&tree, 60);
        assert_eq!(gained, vec![("gathering".to_owned(), 1)]);
    }

    #[test]
    fn a_node_needs_its_parent_and_its_points() {
        let tree = tree();
        let mut state = PlayerState::default();
        assert_eq!(
            state.unlock(&tree, "adventure", "vitality_2"),
            Err(Refused::Needs("vitality_1".into()))
        );
        assert_eq!(
            state.unlock(&tree, "adventure", "vitality_1"),
            Err(Refused::Points {
                needed: 1,
                available: 0
            })
        );
        state.add_xp(&tree, 120);
        let reward = state.unlock(&tree, "adventure", "vitality_1").unwrap();
        assert_eq!(
            reward,
            Some(Reward {
                attribute: "minecraft:max_health".into(),
                amount: 2.0,
                operation: tree::Operation::AddValue,
            })
        );
        assert!(state.is_unlocked("adventure", "vitality_1"));
        assert_eq!(state.available(tree.category("adventure").unwrap()), 0);
        assert_eq!(
            state.unlock(&tree, "adventure", "vitality_1"),
            Err(Refused::AlreadyUnlocked)
        );
        // The parent is unlocked but the point is spent.
        assert_eq!(
            state.unlock(&tree, "adventure", "vitality_2"),
            Err(Refused::Points {
                needed: 1,
                available: 0
            })
        );
    }

    #[test]
    fn a_reset_refunds_and_names_what_to_take_away() {
        let tree = tree();
        let mut state = PlayerState::default();
        state.add_xp(&tree, 1000);
        state.unlock(&tree, "adventure", "vitality_1").unwrap();
        state.unlock(&tree, "adventure", "vitality_2").unwrap();
        state.unlock(&tree, "gathering", "miner_1").unwrap();
        let removed = state.reset(&tree, "adventure");
        assert_eq!(removed, vec!["vitality_1".to_owned(), "vitality_2".to_owned()]);
        assert!(!state.is_unlocked("adventure", "vitality_1"));
        assert!(state.is_unlocked("gathering", "miner_1"));
        let adventure = tree.category("adventure").unwrap();
        assert_eq!(state.available(adventure), 8);
        assert_eq!(state.progress("adventure").spent, 0);
    }

    #[test]
    fn unknown_names_are_refused() {
        let tree = tree();
        let mut state = PlayerState::default();
        assert_eq!(
            state.unlock(&tree, "nope", "vitality_1"),
            Err(Refused::NoCategory("nope".into()))
        );
        assert_eq!(
            state.unlock(&tree, "adventure", "nope"),
            Err(Refused::NoNode("nope".into()))
        );
    }
}
