use crate::util::{Stats, simba::simd_element_iter};

use super::{NodeLink, TriangleBvh};

impl TriangleBvh {
    pub fn print_tree(&self) {
        todo!()
        // self.print_recursive(0, self.root, &self.bounding_box);
    }

    pub fn print_statistics(&self) {
        let (depth, inner, leaf) = self.statistics_recursive(&self.root);
        println!("Triangle count: {}", self.triangle_shading_data.len());
        println!("Vertex count: {}", self.vertex_data.len());
        println!("Leaf depth: {}", depth);
        println!("Inner node fill: {}", inner);
        println!("Leaf nodes fill: {}", leaf);
    }

    /// Returns (depth stats, inner node fill stats, leaf fill stats)
    fn statistics_recursive(&self, node: &NodeLink) -> (Stats, Stats, Stats) {
        match node {
            super::NodeLink::Null => (Stats::default(), Stats::default(), Stats::default()),
            super::NodeLink::Inner { index } => {
                let node = &self.inner_nodes[*index];

                let mut depth_stats = Stats::default();
                let mut inner_stats = Stats::default();
                let mut leaf_stats = Stats::default();

                let mut child_count = 0;

                for i in 0..super::INNER_NODE_CHILDREN {
                    let child = node.child_links.extract(i);
                    if child == NodeLink::Null {
                        continue;
                    }

                    let (child_depth_stats, child_inner_stats, child_leaf_stats) =
                        self.statistics_recursive(&child);

                    depth_stats = depth_stats.merge(&child_depth_stats);
                    inner_stats = inner_stats.merge(&child_inner_stats);
                    leaf_stats = leaf_stats.merge(&child_leaf_stats);

                    child_count += 1;
                }

                depth_stats.min += 1;
                depth_stats.max += 1;
                depth_stats.avg += 1.0;
                inner_stats.add_sample(child_count);

                (depth_stats, inner_stats, leaf_stats)
            }
            super::NodeLink::Leaf { indices } => {
                let leaf_fill = self.triangle_geometry[indices.into_range()]
                    .iter()
                    .flat_map(|ts| {
                        simd_element_iter(ts[0].is_zero() & ts[1].is_zero() & ts[2].is_zero())
                    })
                    .filter(|x| !*x)
                    .count();
                (
                    Stats::new_single(1),
                    Stats::default(),
                    Stats::new_single(leaf_fill),
                )
            }
        }
    }

    // fn print_recursive(&self, indent: usize, node: CompressedNodeLink, enclosing_box: &WorldBox) {
    //     println!(
    //         "{}- {}{}: {:?}-{:?}",
    //         "  ".repeat(indent),
    //         if node.is_leaf() { "L" } else { "I" },
    //         node.index(),
    //         enclosing_box.min,
    //         enclosing_box.max,
    //     );

    //     let enclosing_box = WorldBox8::splat(enclosing_box.clone());

    //     if node.is_leaf() {
    //         self.leaf_geometry_arena[node.index()].print(indent, &enclosing_box);
    //         return;
    //     }

    //     let node = &self.inner_node_arena[node.index()];
    //     let child_boxes = node.child_bounds.decompress(&enclosing_box);

    //     for (i, child_link) in node.child_links.iter().enumerate() {
    //         let child_box = child_boxes.extract(i);
    //         self.print_recursive(indent + 1, *child_link, &child_box);
    //     }
    // }
}
