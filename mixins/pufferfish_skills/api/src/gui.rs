//! The skill tree's window: where its boxes are, and drawing it.
//!
//! The drawing uses only what the screen hooks give a mod — filled rectangles, the game's text
//! and item icons — so the mod needs no GUI texture of its own.

use horizon_client::mod_api::{Host, HudBatch};

use crate::state::PlayerState;
use crate::skill_tree::Tree;

/// The panel's size in GUI pixels.
pub const PANEL_W: f32 = 360.0;
pub const PANEL_H: f32 = 240.0;
const HEADER_H: f32 = 20.0;
const TAB_H: f32 = 14.0;
const CELL_W: f32 = 36.0;
const CELL_H: f32 = 32.0;
const NODE: f32 = 24.0;
const PAD: f32 = 10.0;
const FOOTER_H: f32 = 52.0;

const C_DIM: u32 = 0xB0000000;
const C_PANEL: u32 = 0xF01C1C24;
const C_BORDER: u32 = 0xFF6C6C88;
const C_TAB: u32 = 0xFF2A2A36;
const C_TAB_ON: u32 = 0xFF3C5A8C;
const C_LOCKED: u32 = 0xFF3A3A46;
const C_READY: u32 = 0xFFB58900;
const C_OWNED: u32 = 0xFF2E7D32;
const C_LINK: u32 = 0xFF50506A;
const C_LINK_OWNED: u32 = 0xFF7FA65A;
const C_TEXT: u32 = 0xFFE8E8F0;
const C_TEXT_DIM: u32 = 0xFF9A9AB0;

/// Where everything of one frame is: the panel, its tabs, its boxes and the pointer's place.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub panel: [f32; 4],
    pub close: [f32; 4],
    /// Each tab's rectangle and the category it picks.
    pub tabs: Vec<([f32; 4], usize)>,
    /// Each box's rectangle and the node's index in the category.
    pub nodes: Vec<([f32; 4], usize)>,
    /// The box under the pointer, as a node index.
    pub hovered: Option<usize>,
    /// The category drawn.
    pub category: usize,
}

/// Lays `category` of `tree` out in `viewport` (the whole window, in GUI pixels).
pub fn layout(
    viewport: [f32; 4],
    tree: &Tree,
    state: &PlayerState,
    category: usize,
    mouse: [f32; 2],
) -> Layout {
    let [vx, vy, vw, vh] = viewport;
    let panel = [
        (vx + (vw - PANEL_W) / 2.0).round(),
        (vy + (vh - PANEL_H) / 2.0).round(),
        PANEL_W,
        PANEL_H,
    ];
    let [px, py, pw, _] = panel;
    let category = category.min(tree.categories.len().saturating_sub(1));
    let mut out = Layout {
        panel,
        close: [px + pw - PAD - 9.0, py + 5.0, 9.0, 9.0],
        ..Layout::default()
    };
    out.category = category;
    let count = tree.categories.len().max(1) as f32;
    let tab_w = (pw - 2.0 * PAD - (count - 1.0) * 2.0) / count;
    for (index, _) in tree.categories.iter().enumerate() {
        out.tabs.push((
            [
                px + PAD + index as f32 * (tab_w + 2.0),
                py + HEADER_H + 2.0,
                tab_w,
                TAB_H,
            ],
            index,
        ));
    }
    let Some(category_ref) = tree.categories.get(category) else {
        return out;
    };
    let top = py + HEADER_H + TAB_H + 8.0;
    for (index, node) in category_ref.nodes.iter().enumerate() {
        let rect = [
            px + PAD + node.x as f32 * CELL_W,
            top + node.y as f32 * CELL_H,
            NODE,
            NODE,
        ];
        if contains(rect, mouse) {
            out.hovered = Some(index);
        }
        out.nodes.push((rect, index));
    }
    let _ = state;
    out
}

fn contains(rect: [f32; 4], at: [f32; 2]) -> bool {
    at[0] >= rect[0] && at[1] >= rect[1] && at[0] < rect[0] + rect[2] && at[1] < rect[1] + rect[3]
}

impl Layout {
    /// The tab under `at`.
    pub fn tab_at(&self, at: [f32; 2]) -> Option<usize> {
        self.tabs
            .iter()
            .find(|(rect, _)| contains(*rect, at))
            .map(|(_, index)| *index)
    }

    /// The box under `at`, as a node index.
    pub fn node_at(&self, at: [f32; 2]) -> Option<usize> {
        self.nodes
            .iter()
            .find(|(rect, _)| contains(*rect, at))
            .map(|(_, index)| *index)
    }

    pub fn close_at(&self, at: [f32; 2]) -> bool {
        contains(self.close, at)
    }
}

fn text(host: &dyn Host, batch: &mut HudBatch, line: &str, x: f32, y: f32, rgb: u32, shadow: bool) {
    host.text(batch, line, [x, y], rgb, shadow);
}

fn centered(host: &dyn Host, batch: &mut HudBatch, line: &str, rect: [f32; 4], rgb: u32) {
    let width = host.text_width(line);
    text(
        host,
        batch,
        line,
        rect[0] + (rect[2] - width) / 2.0,
        rect[1] + (rect[3] - 8.0) / 2.0,
        rgb,
        false,
    );
}

fn boxed(host: &dyn Host, batch: &mut HudBatch, rect: [f32; 4], argb: u32) {
    host.fill(batch, rect, argb);
}

/// Draws the tree. `viewport` is the whole window, as the screen hook gives it.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    host: &dyn Host,
    batch: &mut HudBatch,
    viewport: [f32; 4],
    mouse: [f32; 2],
    tree: &Tree,
    state: &PlayerState,
    category: usize,
    notice: Option<&str>,
) {
    let layout = layout(viewport, tree, state, category, mouse);
    let [px, py, pw, ph] = layout.panel;
    boxed(host, batch, viewport, C_DIM);
    boxed(host, batch, layout.panel, C_PANEL);
    // A border of four thin rectangles.
    boxed(host, batch, [px, py, pw, 1.0], C_BORDER);
    boxed(host, batch, [px, py + ph - 1.0, pw, 1.0], C_BORDER);
    boxed(host, batch, [px, py, 1.0, ph], C_BORDER);
    boxed(host, batch, [px + pw - 1.0, py, 1.0, ph], C_BORDER);
    // The close button.
    boxed(host, batch, layout.close, C_TAB);
    centered(host, batch, "x", layout.close, C_TEXT);

    let title = "Skills";
    text(host, batch, title, px + PAD, py + 6.0, C_TEXT, true);
    let Some(category_ref) = tree.categories.get(layout.category) else {
        centered(host, batch, "no skill tree", layout.panel, C_TEXT_DIM);
        return;
    };
    let points = format!("{} points", state.available(category_ref));
    let points_x = px + pw - PAD - host.text_width(&points);
    text(host, batch, &points, points_x, py + 6.0, C_READY, true);

    for (rect, index) in &layout.tabs {
        let on = *index == layout.category;
        boxed(host, batch, *rect, if on { C_TAB_ON } else { C_TAB });
        if let Some(category) = tree.categories.get(*index) {
            centered(host, batch, &category.name, *rect, C_TEXT);
        }
    }

    // The links between the boxes, then the boxes over them.
    for (index, node) in category_ref.nodes.iter().enumerate() {
        let Some((rect, _)) = layout.nodes.get(index) else {
            continue;
        };
        let (cx, cy) = (rect[0] + NODE / 2.0, rect[1] + NODE / 2.0);
        for parent in &node.parents {
            let Some(parent_index) = category_ref.nodes.iter().position(|n| &n.id == parent) else {
                continue;
            };
            let Some((parent_rect, _)) = layout.nodes.get(parent_index) else {
                continue;
            };
            let (ax, ay) = (parent_rect[0] + NODE / 2.0, parent_rect[1] + NODE / 2.0);
            let owned = state.is_unlocked(&category_ref.id, parent)
                && state.is_unlocked(&category_ref.id, &node.id);
            let color = if owned { C_LINK_OWNED } else { C_LINK };
            // An axis-aligned link: most trees sit on a grid.
            let (x0, x1) = if ax < cx { (ax, cx) } else { (cx, ax) };
            let (y0, y1) = if ay < cy { (ay, cy) } else { (cy, ay) };
            if (ax - cx).abs() < 2.0 {
                boxed(host, batch, [x0 - 1.0, y0, 2.0, y1 - y0], color);
            } else if (ay - cy).abs() < 2.0 {
                boxed(host, batch, [x0, y0 - 1.0, x1 - x0, 2.0], color);
            } else {
                boxed(host, batch, [x0, ay - 1.0, x1 - x0, 2.0], color);
                boxed(host, batch, [cx - 1.0, y0, 2.0, y1 - y0], color);
            }
        }
    }

    for (index, node) in category_ref.nodes.iter().enumerate() {
        let Some((rect, _)) = layout.nodes.get(index) else {
            continue;
        };
        let unlocked = state.is_unlocked(&category_ref.id, &node.id);
        let ready = !unlocked && state.can_unlock(tree, &category_ref.id, &node.id).is_ok();
        let base = if unlocked {
            C_OWNED
        } else if ready {
            C_READY
        } else {
            C_LOCKED
        };
        let hovered = layout.hovered == Some(index);
        boxed(host, batch, *rect, if hovered { C_BORDER } else { base });
        boxed(
            host,
            batch,
            [rect[0] + 1.0, rect[1] + 1.0, NODE - 2.0, NODE - 2.0],
            if hovered { C_READY } else { base },
        );
        host.item(batch, &node.icon, [rect[0] + 4.0, rect[1] + 4.0], 16.0);
    }

    // The footer: the hovered skill, or what the server last said.
    let footer_y = py + ph - FOOTER_H;
    boxed(host, batch, [px + PAD, footer_y - 2.0, pw - 2.0 * PAD, 1.0], C_LINK);
    match layout.hovered.and_then(|index| category_ref.nodes.get(index)) {
        Some(node) => {
            text(host, batch, &node.name, px + PAD, footer_y + 2.0, C_TEXT, false);
            let unlocked = state.is_unlocked(&category_ref.id, &node.id);
            let state_text = if unlocked {
                "unlocked".to_owned()
            } else if state.can_unlock(tree, &category_ref.id, &node.id).is_ok() {
                format!("click to unlock for {} point(s)", node.cost)
            } else {
                format!("{} point(s), needs its earlier skills", node.cost)
            };
            let state_x = px + pw - PAD - host.text_width(&state_text);
            text(host, batch, &state_text, state_x, footer_y + 2.0, C_TEXT_DIM, false);
            text(host, batch, &node.description, px + PAD, footer_y + 13.0, C_TEXT_DIM, false);
        }
        None => {
            text(host, batch, "Hover a skill; click to unlock it.", px + PAD, footer_y + 2.0, C_TEXT_DIM, false);
            text(
                host,
                batch,
                "Esc closes this window.",
                px + PAD,
                footer_y + 13.0,
                C_TEXT_DIM,
                false,
            );
        }
    }
    if let Some(notice) = notice {
        text(host, batch, notice, px + PAD, footer_y + 26.0, C_READY, false);
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
    fn the_panel_is_centered_and_boxes_are_hit() {
        let tree = tree();
        let state = PlayerState::default();
        let first = layout([0.0, 0.0, 854.0, 480.0], &tree, &state, 0, [0.0, 0.0]);
        assert_eq!(first.panel[0], (854.0 - PANEL_W) / 2.0);
        assert_eq!(first.panel[1], (480.0 - PANEL_H) / 2.0);
        assert_eq!(first.tabs.len(), tree.categories.len());
        let (rect, index) = first.nodes[0];
        assert_eq!(index, 0);
        let inside = [rect[0] + 2.0, rect[1] + 2.0];
        let hit = layout([0.0, 0.0, 854.0, 480.0], &tree, &state, 0, inside);
        assert_eq!(hit.hovered, Some(0));
        assert_eq!(hit.node_at(inside), Some(0));
        assert_eq!(hit.node_at([0.0, 0.0]), None);
        // The first tab picks the first category.
        let tab = first.tabs[0].0;
        assert_eq!(hit.tab_at([tab[0] + 1.0, tab[1] + 1.0]), Some(0));
        assert!(hit.close_at([hit.close[0] + 1.0, hit.close[1] + 1.0]));
    }

    #[test]
    fn an_unknown_category_draws_no_boxes() {
        let tree = tree();
        let state = PlayerState::default();
        let last = layout([0.0, 0.0, 800.0, 600.0], &tree, &state, 9, [0.0, 0.0]);
        assert!(last.nodes.is_empty());
        assert_eq!(last.category, tree.categories.len() - 1);
    }
}
