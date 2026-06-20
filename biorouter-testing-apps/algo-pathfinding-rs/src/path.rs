use std::fmt::Debug;

/// The result of a successful pathfinding query.
#[derive(Debug, Clone, PartialEq)]
pub struct PathResult<N: Clone + Debug> {
    /// Ordered sequence of nodes from start to goal.
    pub nodes: Vec<N>,
    /// Total accumulated cost of the path.
    pub total_cost: f64,
}

impl<N: Clone + Debug> PathResult<N> {
    /// Number of edges (hops) in the path.
    pub fn len(&self) -> usize {
        self.nodes.len().saturating_sub(1)
    }

    /// Whether the path is empty (single node or no nodes).
    pub fn is_empty(&self) -> bool {
        self.nodes.len() <= 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_result_len() {
        let p = PathResult {
            nodes: vec![1, 2, 3, 4],
            total_cost: 10.0,
        };
        assert_eq!(p.len(), 3);
        assert!(!p.is_empty());
    }

    #[test]
    fn test_path_result_empty() {
        let p = PathResult {
            nodes: vec![1],
            total_cost: 0.0,
        };
        assert!(p.is_empty());
    }
}
