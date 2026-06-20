use crate::model::{Drug, Interaction, SeverityLevel};
use petgraph::algo::tarjan_scc;
use petgraph::graph::{NodeIndex, UnGraph};
use std::collections::{HashMap, HashSet, VecDeque};

/// The core graph engine wrapping a petgraph graph.
pub struct InteractionGraph {
    /// Undirected graph for traversal / analysis
    pub graph: UnGraph<String, WeightedEdge>,
    /// Map from drug name to node index
    pub node_map: HashMap<String, NodeIndex>,
    /// Map from node index to drug name
    pub idx_map: HashMap<NodeIndex, String>,
    /// Map from (canonical drug pair) to the Interaction record
    pub interaction_map: HashMap<(String, String), Interaction>,
}

/// Weighted edge data carried on graph edges.
#[derive(Debug, Clone)]
pub struct WeightedEdge {
    pub severity: SeverityLevel,
    pub interaction_type: crate::model::InteractionType,
    pub mechanism: String,
}

impl InteractionGraph {
    /// Build the graph from drugs and interactions.
    pub fn new(drugs: &[Drug], interactions: &[Interaction]) -> Self {
        let graph = UnGraph::<String, WeightedEdge>::new_undirected();
        let mut ig = InteractionGraph {
            graph,
            node_map: HashMap::new(),
            idx_map: HashMap::new(),
            interaction_map: HashMap::new(),
        };

        // Add all drugs as nodes
        for drug in drugs {
            ig.add_drug_node(&drug.name);
        }

        // Add all interactions as edges
        for interaction in interactions {
            // Ensure nodes exist (drugs might appear only in interactions)
            ig.add_drug_node(&interaction.drug_a);
            ig.add_drug_node(&interaction.drug_b);

            let pair = interaction.pair();
            let key = (pair.0.to_string(), pair.1.to_string());

            ig.interaction_map.insert(key, interaction.clone());

            let node_a = ig.node_map[&interaction.drug_a];
            let node_b = ig.node_map[&interaction.drug_b];

            let edge_data = WeightedEdge {
                severity: interaction.severity,
                interaction_type: interaction.interaction_type,
                mechanism: interaction.mechanism.clone(),
            };

            ig.graph.add_edge(node_a, node_b, edge_data);
        }

        ig
    }

    fn add_drug_node(&mut self, name: &str) -> NodeIndex {
        if let Some(&idx) = self.node_map.get(name) {
            idx
        } else {
            let idx = self.graph.add_node(name.to_string());
            self.node_map.insert(name.to_string(), idx);
            self.idx_map.insert(idx, name.to_string());
            idx
        }
    }

    /// Get all drugs (names) in the graph.
    #[allow(dead_code)]
    pub fn all_drugs(&self) -> Vec<&str> {
        self.node_map.keys().map(|s| s.as_str()).collect()
    }

    /// Get all interactions in the graph.
    #[allow(dead_code)]
    pub fn all_interactions(&self) -> Vec<&Interaction> {
        self.interaction_map.values().collect()
    }

    // ─── Graph Algorithms ───────────────────────────────────────────────────

    /// Get direct neighbors of a drug (all drugs that interact with it).
    pub fn neighbors(&self, drug_name: &str) -> Vec<String> {
        let drug_lower = drug_name.to_lowercase();
        if let Some(&idx) = self.node_map.get(&drug_lower) {
            self.graph
                .neighbors(idx)
                .map(|n| self.idx_map[&n].clone())
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Get all interactions involving a specific drug.
    pub fn interactions_for(&self, drug_name: &str) -> Vec<&Interaction> {
        let drug_lower = drug_name.to_lowercase();
        self.interaction_map
            .values()
            .filter(|ix| ix.drug_a == drug_lower || ix.drug_b == drug_lower)
            .collect()
    }

    /// Find the shortest path between two drugs (fewest edges).
    /// Returns Some(Vec<drug_names>) including start and end, or None.
    pub fn shortest_path(&self, from: &str, to: &str) -> Option<Vec<String>> {
        let from_lower = from.to_lowercase();
        let to_lower = to.to_lowercase();

        let start = *self.node_map.get(&from_lower)?;
        let end = *self.node_map.get(&to_lower)?;

        // BFS for unweighted shortest path
        let mut visited: HashSet<NodeIndex> = HashSet::new();
        let mut parent: HashMap<NodeIndex, NodeIndex> = HashMap::new();
        let mut queue = VecDeque::new();

        visited.insert(start);
        queue.push_back(start);

        while let Some(current) = queue.pop_front() {
            if current == end {
                // Reconstruct path
                let mut path = Vec::new();
                let mut node = end;
                path.push(self.idx_map[&node].clone());
                while let Some(&p) = parent.get(&node) {
                    path.push(self.idx_map[&p].clone());
                    node = p;
                }
                path.reverse();
                return Some(path);
            }

            for neighbor in self.graph.neighbors(current) {
                if !visited.contains(&neighbor) {
                    visited.insert(neighbor);
                    parent.insert(neighbor, current);
                    queue.push_back(neighbor);
                }
            }
        }

        None
    }

    /// Find connected components (interaction clusters) in the graph.
    /// Returns a Vec of Vecs, each inner Vec is a cluster of drug names.
    pub fn connected_components(&self) -> Vec<Vec<String>> {
        // Use tarjan SCC on the undirected graph (gives connected components)
        let sccs = tarjan_scc(&self.graph);
        sccs.into_iter()
            .map(|scc| scc.into_iter().map(|idx| self.idx_map[&idx].clone()).collect())
            .collect()
    }

    /// Calculate degree centrality for each drug.
    /// Returns (drug_name, degree) sorted by descending degree.
    pub fn degree_centrality(&self) -> Vec<(String, usize)> {
        let mut centrality: Vec<(String, usize)> = self
            .node_map
            .iter()
            .map(|(name, &idx)| {
                let degree = self.graph.edges(idx).count();
                (name.clone(), degree)
            })
            .collect();
        centrality.sort_by(|a, b| b.1.cmp(&a.1));
        centrality
    }

    /// Calculate weighted degree centrality using severity as weight.
    /// Higher sum means more dangerous hub.
    pub fn weighted_centrality(&self) -> Vec<(String, u32)> {
        let mut centrality: Vec<(String, u32)> = self
            .node_map
            .iter()
            .map(|(name, &idx)| {
                let weight_sum: u32 = self
                    .graph
                    .edges(idx)
                    .map(|e| e.weight().severity.score())
                    .sum();
                (name.clone(), weight_sum)
            })
            .collect();
        centrality.sort_by(|a, b| b.1.cmp(&a.1));
        centrality
    }

    /// Find all interaction chains (paths) between drugs in a given set.
    /// Chains must have length >= 3 (at least one intermediate drug).
    pub fn find_chains(&self, drug_set: &[String], max_chain_len: usize) -> Vec<Vec<String>> {
        let drug_set_lower: HashSet<String> = drug_set.iter().map(|d| d.to_lowercase()).collect();
        let mut chains = Vec::new();

        // For each pair of drugs in the set, find shortest path
        let drugs_in_graph: Vec<&str> = drug_set_lower
            .iter()
            .filter(|d| self.node_map.contains_key(d.as_str()))
            .map(|d| d.as_str())
            .collect();

        for i in 0..drugs_in_graph.len() {
            for j in (i + 1)..drugs_in_graph.len() {
                if let Some(path) = self.shortest_path(drugs_in_graph[i], drugs_in_graph[j]) {
                    if path.len() >= 3 && path.len() <= max_chain_len {
                        chains.push(path);
                    }
                }
            }
        }

        chains
    }

    /// Detect "hub" drugs: drugs with weighted centrality above the given percentile.
    pub fn find_hub_drugs(&self, percentile: f64) -> Vec<(String, u32)> {
        let centrality = self.weighted_centrality();
        if centrality.is_empty() {
            return Vec::new();
        }

        let threshold_idx = ((centrality.len() as f64) * (1.0 - percentile)) as usize;
        let threshold_idx = threshold_idx.min(centrality.len() - 1);
        let threshold = centrality[threshold_idx].1;

        centrality
            .into_iter()
            .filter(|(_, score)| *score >= threshold && *score > 0)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EvidenceLevel, InteractionType};

    fn test_graph() -> InteractionGraph {
        let drugs = vec![
            Drug::new("warfarin", "anticoagulant", vec!["VKORC1".into()]),
            Drug::new("aspirin", "nsaid", vec!["COX-1".into()]),
            Drug::new("fluoxetine", "ssri", vec!["SERT".into()]),
            Drug::new("omeprazole", "ppi", vec!["CYP2C19".into()]),
            Drug::new("metformin", "biguanide", vec!["AMPK".into()]),
        ];

        let interactions = vec![
            Interaction {
                drug_a: "aspirin".into(),
                drug_b: "warfarin".into(),
                interaction_type: InteractionType::Pharmacodynamic,
                severity: SeverityLevel::Major,
                mechanism: "Additive anticoagulation".into(),
                evidence: EvidenceLevel::Established,
                recommendation: None,
            },
            Interaction {
                drug_a: "fluoxetine".into(),
                drug_b: "warfarin".into(),
                interaction_type: InteractionType::Pharmacokinetic,
                severity: SeverityLevel::Moderate,
                mechanism: "CYP2C9 inhibition".into(),
                evidence: EvidenceLevel::Probable,
                recommendation: None,
            },
            Interaction {
                drug_a: "omeprazole".into(),
                drug_b: "fluoxetine".into(),
                interaction_type: InteractionType::Pharmacokinetic,
                severity: SeverityLevel::Minor,
                mechanism: "CYP2C19 effect".into(),
                evidence: EvidenceLevel::Suspected,
                recommendation: None,
            },
        ];

        InteractionGraph::new(&drugs, &interactions)
    }

    #[test]
    fn test_graph_construction() {
        let ig = test_graph();
        assert_eq!(ig.node_map.len(), 5);
        assert_eq!(ig.interaction_map.len(), 3);
    }

    #[test]
    fn test_neighbors() {
        let ig = test_graph();
        let neighbors = ig.neighbors("warfarin");
        assert_eq!(neighbors.len(), 2);
        assert!(neighbors.contains(&"aspirin".to_string()));
        assert!(neighbors.contains(&"fluoxetine".to_string()));
    }

    #[test]
    fn test_interactions_for() {
        let ig = test_graph();
        let ix = ig.interactions_for("warfarin");
        assert_eq!(ix.len(), 2);
    }

    #[test]
    fn test_shortest_path_direct() {
        let ig = test_graph();
        let path = ig.shortest_path("warfarin", "aspirin").unwrap();
        assert_eq!(path, vec!["warfarin", "aspirin"]);
    }

    #[test]
    fn test_shortest_path_indirect() {
        let ig = test_graph();
        // warfarin -> fluoxetine -> omeprazole (via intermediate)
        let path = ig.shortest_path("warfarin", "omeprazole").unwrap();
        assert!(path.len() >= 3);
        assert_eq!(path[0], "warfarin");
        assert_eq!(path[path.len() - 1], "omeprazole");
    }

    #[test]
    fn test_shortest_path_none() {
        let ig = test_graph();
        // metformin has no interactions in test_graph
        let path = ig.shortest_path("warfarin", "metformin");
        assert!(path.is_none());
    }

    #[test]
    fn test_connected_components() {
        let ig = test_graph();
        let components = ig.connected_components();
        // Should have 2 components: {warfarin, aspirin, fluoxetine, omeprazole} and {metformin}
        assert_eq!(components.len(), 2);
        let largest = components.iter().max_by_key(|c| c.len()).unwrap();
        assert_eq!(largest.len(), 4);
    }

    #[test]
    fn test_degree_centrality() {
        let ig = test_graph();
        let centrality = ig.degree_centrality();
        assert_eq!(centrality.len(), 5);
        // warfarin should have highest degree (2)
        let warfarin_entry = centrality.iter().find(|(name, _)| name == "warfarin").unwrap();
        assert_eq!(warfarin_entry.1, 2);
        // All top entries should have degree 2
        assert_eq!(centrality[0].1, 2);
        assert_eq!(centrality[1].1, 2);
    }

    #[test]
    fn test_weighted_centrality() {
        let ig = test_graph();
        let centrality = ig.weighted_centrality();
        assert_eq!(centrality.len(), 5);
        // warfarin: Major(3) + Moderate(2) = 5
        assert_eq!(centrality[0].0, "warfarin");
        assert_eq!(centrality[0].1, 5);
    }

    #[test]
    fn test_find_chains() {
        let ig = test_graph();
        // warfarin, fluoxetine, omeprazole are in a chain
        let chain_drugs = vec![
            "warfarin".to_string(),
            "fluoxetine".to_string(),
            "omeprazole".to_string(),
        ];
        let chains = ig.find_chains(&chain_drugs, 10);
        assert!(!chains.is_empty());
    }

    #[test]
    fn test_find_hub_drugs() {
        let ig = test_graph();
        let hubs = ig.find_hub_drugs(0.5);
        assert!(!hubs.is_empty());
        // warfarin should be in the hubs
        assert!(hubs.iter().any(|(name, _)| name == "warfarin"));
    }
}
