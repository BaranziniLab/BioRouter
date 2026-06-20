/// Heuristic distance functions for A* and similar informed search algorithms.
///
/// All functions return `f64` and satisfy the triangle inequality.
/// Manhattan (L1) distance between two 2D grid points.
pub fn manhattan(a: &(i32, i32), b: &(i32, i32)) -> f64 {
    ((a.0 - b.0).abs() + (a.1 - b.1).abs()) as f64
}

/// Euclidean (L2) distance between two 2D grid points.
pub fn euclidean(a: &(i32, i32), b: &(i32, i32)) -> f64 {
    let dx = (a.0 - b.0) as f64;
    let dy = (a.1 - b.1) as f64;
    (dx * dx + dy * dy).sqrt()
}

/// Chebyshev (L∞) distance — appropriate for 8-connected grids.
pub fn chebyshev(a: &(i32, i32), b: &(i32, i32)) -> f64 {
    ((a.0 - b.0).abs()).max((a.1 - b.1).abs()) as f64
}

/// Octile distance — blends Manhattan and Chebyshev for 8-connected grids
/// where diagonal moves cost √2.
pub fn octile(a: &(i32, i32), b: &(i32, i32)) -> f64 {
    let dx = (a.0 - b.0).abs() as f64;
    let dy = (a.1 - b.1).abs() as f64;
    let diag = dx.min(dy);
    let straight = (dx - dy).abs();
    diag * std::f64::consts::SQRT_2 + straight
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manhattan() {
        let a = (0, 0);
        let b = (3, 4);
        assert!((manhattan(&a, &b) - 7.0).abs() < 1e-9);
        assert!((manhattan(&b, &a) - 7.0).abs() < 1e-9);
    }

    #[test]
    fn test_euclidean() {
        let a = (0, 0);
        let b = (3, 4);
        assert!((euclidean(&a, &b) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn test_chebyshev() {
        let a = (0, 0);
        let b = (3, 4);
        assert!((chebyshev(&a, &b) - 4.0).abs() < 1e-9);
    }

    #[test]
    fn test_octile() {
        let a = (0, 0);
        let b = (3, 3);
        // All diagonal: 3 * sqrt(2)
        let expected = 3.0 * std::f64::consts::SQRT_2;
        assert!((octile(&a, &b) - expected).abs() < 1e-9);
    }

    #[test]
    fn test_octile_mixed() {
        let a = (0, 0);
        let b = (3, 1);
        // 1 diagonal (sqrt2) + 2 straight = sqrt(2) + 2
        let expected = std::f64::consts::SQRT_2 + 2.0;
        assert!((octile(&a, &b) - expected).abs() < 1e-9);
    }

    #[test]
    fn test_heuristics_admissible_for_manhattan_grid() {
        // On a 4-connected grid the true cost is the Manhattan distance.
        // All heuristics must be <= true cost for admissibility.
        let a = (0, 0);
        let b = (5, 3);
        let true_cost = manhattan(&a, &b);
        assert!(manhattan(&a, &b) <= true_cost + 1e-9);
        assert!(euclidean(&a, &b) <= true_cost + 1e-9);
        assert!(chebyshev(&a, &b) <= true_cost + 1e-9);
        assert!(octile(&a, &b) <= true_cost + 1e-9);
    }
}
