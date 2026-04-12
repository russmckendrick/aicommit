use std::collections::BTreeMap;

use svg::Document;

use super::{
    palette::directory_colour,
    svg_util::{
        group, new_document, rect, rounded_rect, subtitle_text, text, title_text, truncate_to_width,
    },
};

/// A node in the file tree. Leaf nodes have a size (line count); directories sum children.
#[derive(Debug, Clone)]
pub struct TreeNode {
    pub name: String,
    pub size: f64,
    pub children: Vec<TreeNode>,
}

impl TreeNode {
    pub fn total_size(&self) -> f64 {
        if self.children.is_empty() {
            self.size
        } else {
            self.children.iter().map(|c| c.total_size()).sum()
        }
    }
}

/// Build a tree from a map of file paths to line counts.
pub fn build_tree(files: &BTreeMap<String, usize>) -> TreeNode {
    let mut root = TreeNode {
        name: String::new(),
        size: 0.0,
        children: Vec::new(),
    };
    for (path, &lines) in files {
        let parts: Vec<&str> = path.split('/').collect();
        insert_path(&mut root, &parts, lines as f64);
    }
    collapse_single_children(&mut root);
    root
}

fn insert_path(node: &mut TreeNode, parts: &[&str], size: f64) {
    if parts.is_empty() {
        return;
    }
    if parts.len() == 1 {
        node.children.push(TreeNode {
            name: parts[0].to_owned(),
            size,
            children: Vec::new(),
        });
        return;
    }
    let dir_name = parts[0];
    let child = node
        .children
        .iter_mut()
        .find(|c| c.name == dir_name && !c.children.is_empty());
    match child {
        Some(existing) => insert_path(existing, &parts[1..], size),
        None => {
            let mut new_dir = TreeNode {
                name: dir_name.to_owned(),
                size: 0.0,
                children: Vec::new(),
            };
            insert_path(&mut new_dir, &parts[1..], size);
            node.children.push(new_dir);
        }
    }
}

fn collapse_single_children(node: &mut TreeNode) {
    for child in &mut node.children {
        collapse_single_children(child);
    }
    if node.children.len() == 1 && !node.children[0].children.is_empty() {
        let child = node.children.remove(0);
        let new_name = if node.name.is_empty() {
            child.name
        } else {
            format!("{}/{}", node.name, child.name)
        };
        node.name = new_name;
        node.children = child.children;
    }
}

/// Render the tree as an SVG treemap.
pub fn render(tree: &TreeNode, label: Option<&str>) -> Document {
    let padding = 40.0;
    let header_height = 60.0;
    let map_width = 960.0;
    let map_height = 540.0;
    let total_width = map_width + padding * 2.0;
    let total_height = map_height + header_height + padding * 2.0;

    let mut doc = new_document(total_width, total_height);

    // Background
    doc = doc.add(rect(0.0, 0.0, total_width, total_height, "#fafafa"));

    // Title
    let title = label.unwrap_or("Codebase Treemap");
    doc = doc.add(title_text(padding, padding + 20.0, title));
    let file_count = count_leaves(tree);
    let total_lines = tree.total_size() as usize;
    doc = doc.add(subtitle_text(
        padding,
        padding + 38.0,
        &format!("{file_count} files, {total_lines} lines"),
    ));

    let map_x = padding;
    let map_y = header_height + padding;

    // Render top-level directories as coloured groups
    let top_children = if tree.children.is_empty() {
        vec![tree.clone()]
    } else {
        tree.children.clone()
    };

    let rects = squarify(&top_children, map_x, map_y, map_width, map_height);

    for (i, (node, rx, ry, rw, rh)) in rects.iter().enumerate() {
        let colour = directory_colour(i);
        let g = render_node(node, *rx, *ry, *rw, *rh, &colour, 0);
        doc = doc.add(g);
    }

    doc
}

fn render_node(
    node: &TreeNode,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    base_colour: &str,
    depth: usize,
) -> svg::node::element::Group {
    let mut g = group();
    let gap = 2.0;

    if node.children.is_empty() {
        // Leaf node
        g = g.add(
            rounded_rect(
                x + gap,
                y + gap,
                (w - gap * 2.0).max(0.0),
                (h - gap * 2.0).max(0.0),
                base_colour,
                3.0,
            )
            .set("opacity", 0.85)
            .set("stroke", "#ffffff")
            .set("stroke-width", 1),
        );
        if w > 30.0 && h > 14.0 {
            let label = truncate_to_width(&node.name, w - gap * 4.0, 10.0);
            g = g.add(text(x + gap + 4.0, y + gap + 13.0, &label, 10.0).set("fill", "#1a1a1a"));
        }
    } else {
        // Directory container
        let label_height = if depth == 0 && h > 20.0 { 18.0 } else { 0.0 };
        g = g.add(
            rounded_rect(
                x + gap,
                y + gap,
                (w - gap * 2.0).max(0.0),
                (h - gap * 2.0).max(0.0),
                base_colour,
                4.0,
            )
            .set("opacity", 0.15),
        );
        if label_height > 0.0 && w > 40.0 {
            let label = truncate_to_width(&node.name, w - gap * 4.0, 11.0);
            g = g.add(
                text(x + gap + 4.0, y + gap + 13.0, &label, 11.0)
                    .set("fill", "#1a1a1a")
                    .set("font-weight", "600"),
            );
        }
        let child_rects = squarify(
            &node.children,
            x + gap,
            y + gap + label_height,
            (w - gap * 2.0).max(0.0),
            (h - gap * 2.0 - label_height).max(0.0),
        );
        for (child, cx, cy, cw, ch) in &child_rects {
            g = g.add(render_node(
                child,
                *cx,
                *cy,
                *cw,
                *ch,
                base_colour,
                depth + 1,
            ));
        }
    }

    g
}

/// Squarified treemap layout: partition children into rows that minimise aspect ratio.
fn squarify(
    nodes: &[TreeNode],
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) -> Vec<(&TreeNode, f64, f64, f64, f64)> {
    if nodes.is_empty() || w <= 0.0 || h <= 0.0 {
        return Vec::new();
    }

    let total: f64 = nodes.iter().map(|n| n.total_size()).sum();
    if total <= 0.0 {
        return Vec::new();
    }

    // Sort by size descending
    let mut sorted: Vec<(usize, f64)> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (i, n.total_size()))
        .collect();
    sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut result = Vec::new();
    let mut remaining_area = w * h;
    let mut cx = x;
    let mut cy = y;
    let mut cw = w;
    let mut ch = h;
    let mut remaining_total = total;

    let mut i = 0;
    while i < sorted.len() {
        let is_wide = cw >= ch;
        let side = if is_wide { ch } else { cw };

        // Greedily add items to the current row while aspect ratio improves
        let mut row = vec![sorted[i]];
        let mut row_sum = sorted[i].1;
        let mut best_worst_ratio =
            worst_ratio(&row, row_sum, remaining_total, side, remaining_area);

        let mut j = i + 1;
        while j < sorted.len() {
            let mut candidate = row.clone();
            candidate.push(sorted[j]);
            let candidate_sum = row_sum + sorted[j].1;
            let candidate_ratio = worst_ratio(
                &candidate,
                candidate_sum,
                remaining_total,
                side,
                remaining_area,
            );
            if candidate_ratio <= best_worst_ratio {
                row = candidate;
                row_sum = candidate_sum;
                best_worst_ratio = candidate_ratio;
                j += 1;
            } else {
                break;
            }
        }

        // Layout the row
        let row_fraction = if remaining_total > 0.0 {
            row_sum / remaining_total
        } else {
            1.0
        };
        let row_size = if is_wide {
            cw * row_fraction
        } else {
            ch * row_fraction
        };

        let mut offset = 0.0;
        for &(idx, size) in &row {
            let item_fraction = if row_sum > 0.0 { size / row_sum } else { 1.0 };
            let item_size = side * item_fraction;
            let (rx, ry, rw, rh) = if is_wide {
                (cx, cy + offset, row_size, item_size)
            } else {
                (cx + offset, cy, item_size, row_size)
            };
            result.push((&nodes[idx], rx, ry, rw, rh));
            offset += item_size;
        }

        // Shrink remaining area
        if is_wide {
            cx += row_size;
            cw -= row_size;
        } else {
            cy += row_size;
            ch -= row_size;
        }
        remaining_total -= row_sum;
        remaining_area = cw * ch;
        i = j;
    }

    result
}

fn worst_ratio(row: &[(usize, f64)], row_sum: f64, total: f64, side: f64, area: f64) -> f64 {
    if row.is_empty() || total <= 0.0 || area <= 0.0 || side <= 0.0 {
        return f64::MAX;
    }
    let row_area = area * (row_sum / total);
    let row_length = row_area / side;
    let mut worst = 0.0_f64;
    for &(_, size) in row {
        let item_area = area * (size / total);
        let item_width = if row_length > 0.0 {
            item_area / row_length
        } else {
            0.0
        };
        let ratio = if item_width > 0.0 && row_length > 0.0 {
            (row_length / item_width).max(item_width / row_length)
        } else {
            f64::MAX
        };
        worst = worst.max(ratio);
    }
    worst
}

fn count_leaves(node: &TreeNode) -> usize {
    if node.children.is_empty() {
        1
    } else {
        node.children.iter().map(count_leaves).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_tree_groups_by_directory() {
        let mut files = BTreeMap::new();
        files.insert("src/main.rs".to_owned(), 100);
        files.insert("src/lib.rs".to_owned(), 50);
        files.insert("README.md".to_owned(), 20);
        let tree = build_tree(&files);
        assert_eq!(tree.total_size(), 170.0);
    }

    #[test]
    fn squarify_produces_correct_count() {
        let nodes = vec![
            TreeNode {
                name: "a".into(),
                size: 60.0,
                children: vec![],
            },
            TreeNode {
                name: "b".into(),
                size: 30.0,
                children: vec![],
            },
            TreeNode {
                name: "c".into(),
                size: 10.0,
                children: vec![],
            },
        ];
        let rects = squarify(&nodes, 0.0, 0.0, 100.0, 100.0);
        assert_eq!(rects.len(), 3);
    }
}
