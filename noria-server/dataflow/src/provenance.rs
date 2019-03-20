use fnv::FnvHashMap;
use petgraph::graph::NodeIndex;
use prelude::*;

/// The upstream branch of nodes and message labels that was updated to produce the current
/// message, starting at the node above the payload's "from" node. The number of nodes in the
/// update is linear in the depth of the update.
pub type ProvenanceUpdate = Vec<(NodeIndex, usize)>;

/// The history of message labels that correspond to the production of the current message.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Provenance {
    root: NodeIndex,
    label: usize,
    edges: FnvHashMap<NodeIndex, Box<Provenance>>,
}

impl Default for Provenance {
    // TODO(ygina): it doesn't really make sense to have a provenance for imaginary node index 0,
    // so maybe we should use options here. this is hacky and gross. the reason we have a default
    // implementation is hack to intiialize the provenance graph in the egress AFTER it has
    // been otherwise initialized and fit into the graph.
    fn default() -> Provenance {
        Provenance {
            root: NodeIndex::new(0),
            edges: Default::default(),
            label: 0,
        }
    }
}

impl Provenance {
    /// Initializes the provenance graph from the root node up to the given depth.
    /// Typically called on a default Provenance struct, compared to an empty one.
    pub fn init(&mut self, graph: &Graph, root: NodeIndex, depth: usize) {
        self.root = root;
        if depth > 0 {
            // TODO(ygina): operate on domain level instead of ingress/egress level
            let mut egresses = Vec::new();
            let mut queue = Vec::new();
            queue.push(root);
            while queue.len() > 0 {
                let ni = queue.pop().unwrap();
                if graph[ni].is_egress() && ni != root {
                    egresses.push(ni);
                    continue;
                }

                let mut children = graph
                    .neighbors_directed(ni, petgraph::EdgeDirection::Incoming)
                    .collect::<Vec<_>>();
                queue.append(&mut children);
            }

            for egress in egresses {
                let mut provenance = Provenance::default();
                provenance.init(graph, egress, depth - 1);
                self.edges.insert(egress, box provenance);
            }
        }
    }

    /// Constructs an empty, uninitialized provenance graph for the given node and label
    pub fn empty(node: NodeIndex, label: usize) -> Provenance {
        Provenance {
            root: node,
            edges: FnvHashMap::default(),
            label,
        }
    }

    pub fn label(&self) -> usize {
        self.label
    }

    pub fn set_label(&mut self, label: usize) {
        self.label = label;
    }

    /// Apply a single provenance update cached from the message payloads. In general, this method
    /// is called only when the full provenance is necessary, as in when we need to make recovery.
    pub fn apply_update(&mut self, update: &ProvenanceUpdate) {
        self.label += 1;
        let mut provenance = self;
        for (node, label) in update {
            if let Some(p) = provenance.edges.get_mut(node) {
                p.set_label(*label);
                provenance = p;
            } else {
                break;
            }
        }
    }

    pub fn apply_updates(&mut self, updates: &[ProvenanceUpdate]) {
        for update in updates {
            self.apply_update(update);
        }
    }

    /// Subgraph of this provenance graph with the given node as the new root. The new root must be
    /// an ancestor (stateless domain recovery) or grand-ancestor (stateful domain recovery) of the
    /// given node. There's no reason we should obtain any other subgraph in the recovery protocol.
    pub fn subgraph(&self, new_root: NodeIndex) -> &Box<Provenance> {
        if let Some(p) = self.edges.get(&new_root) {
            return p;
        }
        // replicas
        for (_, p) in &self.edges {
            if let Some(p) = p.edges.get(&new_root){
                return p;
            }
        }
        unreachable!("must be ancestor or grand-ancestor");
    }
}
