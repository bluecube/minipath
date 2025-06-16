use simba::simd::SimdValue as _;

use crate::{
    geometry::{WorldBox, WorldBox8},
    util::{Stats, simba::simd_element_iter},
};

use super::{LEAF_NODE_TRIANGLES, LeafGeometry, NodeLink, TriangleBvh};

use assert2::assert;

impl TriangleBvh {
    pub fn print_tree(&self) {
        self.print_recursive(0, self.root, &self.bounding_box);
    }

    pub fn print_statistics(&self) {
        let depth = self.depth_statistics_recursive(self.root);
        let inner = self.inner_node_fill_statistics();
        let leaf = self.leaf_node_fill_statistics();
        println!("Depth: {} - {}; avg {:.1}", depth.min, depth.max, depth.avg);
        println!("Inner node child count: {}", inner);
        println!("Leaf nodes fill: {}", leaf);
    }

    fn depth_statistics_recursive(&self, node: NodeLink) -> Stats {
        if node.is_leaf() {
            return Stats::new_single(1);
        }

        let node = &self.inner_node_arena[node.index()];

        let mut ret = node
            .child_links
            .iter()
            .map(|child| self.depth_statistics_recursive(*child))
            .reduce(|a, b| a.merge(&b))
            .unwrap();

        ret.min += 1;
        ret.max += 1;
        ret.avg += 1.0;

        ret
    }

    fn inner_node_fill_statistics(&self) -> Stats {
        let mut stats = Stats::default();

        stats.add_samples(self.inner_node_arena.iter().map(|child| {
            child
                .child_links
                .iter()
                .filter(|link| !link.is_leaf() || link.index() != 0)
                .count()
        }));

        stats
    }

    fn leaf_node_fill_statistics(&self) -> Stats {
        let mut stats = Stats::default();

        stats.add_samples(
            self.leaf_geometry_arena
                .iter()
                .map(|leaf| leaf.used_triangles()),
        );

        stats
    }

    fn print_recursive(&self, indent: usize, node: NodeLink, enclosing_box: &WorldBox) {
        println!(
            "{}- {}{}: {:?}-{:?}",
            "  ".repeat(indent),
            if node.is_leaf() { "L" } else { "I" },
            node.index(),
            enclosing_box.min,
            enclosing_box.max,
        );

        let enclosing_box = WorldBox8::splat(enclosing_box.clone());

        if node.is_leaf() {
            self.leaf_geometry_arena[node.index()].print(indent, &enclosing_box);
            return;
        }

        let node = &self.inner_node_arena[node.index()];
        let child_boxes = node.child_bounds.decompress(&enclosing_box);

        for (i, child_link) in node.child_links.iter().enumerate() {
            let child_box = child_boxes.extract(i);
            self.print_recursive(indent + 1, *child_link, &child_box);
        }
    }
}

impl LeafGeometry {
    fn print(&self, indent: usize, enclosing_box: &WorldBox8) {
        let indent = "  ".repeat(indent + 1);

        let mut empty_count = 0;
        for (empty, triangle) in self.triangles.iter().flat_map(|ts| {
            let empty = ts[0].is_zero() & ts[1].is_zero() & ts[2].is_zero();
            let decompressed = ts.decompress(enclosing_box);
            simd_element_iter(empty).zip(simd_element_iter(decompressed))
        }) {
            if empty {
                empty_count += 1;
            } else {
                assert!(empty_count == 0);
                println!(
                    "{}{:?}, {:?}, {:?}",
                    indent, triangle[0], triangle[1], triangle[2]
                )
            }
        }

        if empty_count > 0 {
            println!("{}{}x <EMPTY>", indent, empty_count);
        }
    }

    fn used_triangles(&self) -> usize {
        LEAF_NODE_TRIANGLES
            - self
                .triangles
                .iter()
                .flat_map(|ts| {
                    let empty = ts[0].is_zero() & ts[1].is_zero() & ts[2].is_zero();
                    simd_element_iter(empty)
                })
                .filter(|x| *x)
                .count()
    }
}
