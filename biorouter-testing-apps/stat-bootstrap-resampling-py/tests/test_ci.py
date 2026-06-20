"""
Tests for confidence intervals module.
"""

import numpy as np
import pytest
from resampling import (
    bootstrap_ci,
    percentile_ci,
    basic_ci,
    bca_ci,
    bootstrap_t_ci,
)


class TestPercentileCI:
    """Tests for percentile confidence interval."""
    
    def test_basic(self):
        """Test basic percentile CI functionality."""
        np.random.seed(42)
        data = np.random.normal(0, 1, 100)
        
        result = bootstrap_ci(data, np.mean, method='percentile', B=999, seed=42)
        
        assert result.ci_lower < result.estimate < result.ci_upper
        assert result.ci_level == 0.95
    
    def test_coverage(self):
        """Test that percentile CI has approximately nominal coverage."""
        np.random.seed(42)
        n_trials = 200
        n_covered = 0
        true_mean = 5.0
        
        for i in range(n_trials):
            data = np.random.normal(true_mean, 1, 50)
            result = bootstrap_ci(data, np.mean, method='percentile', B=999, seed=i)
            
            if result.ci_lower <= true_mean <= result.ci_upper:
                n_covered += 1
        
        coverage = n_covered / n_trials
        
        # Should be close to 95% (within tolerance for finite samples)
        assert 0.85 <= coverage <= 1.0
    
    def test_ci_width(self):
        """Test CI width decreases with sample size."""
        np.random.seed(42)
        
        # Small sample
        data_small = np.random.normal(0, 1, 30)
        result_small = bootstrap_ci(data_small, np.mean, method='percentile', B=999, seed=42)
        
        # Large sample
        data_large = np.random.normal(0, 1, 300)
        result_large = bootstrap_ci(data_large, np.mean, method='percentile', B=999, seed=42)
        
        # CI should be narrower for larger sample
        assert result_large.ci_width() < result_small.ci_width()


class TestBasicCI:
    """Tests for basic (pivotal) confidence interval."""
    
    def test_basic(self):
        """Test basic CI functionality."""
        np.random.seed(42)
        data = np.random.normal(0, 1, 100)
        
        result = bootstrap_ci(data, np.mean, method='basic', B=999, seed=42)
        
        assert result.ci_lower < result.estimate < result.ci_upper
        assert result.method == 'basic'
    
    def test_coverage(self):
        """Test that basic CI has approximately nominal coverage."""
        np.random.seed(42)
        n_trials = 200
        n_covered = 0
        true_mean = 5.0
        
        for i in range(n_trials):
            data = np.random.normal(true_mean, 1, 50)
            result = bootstrap_ci(data, np.mean, method='basic', B=999, seed=i)
            
            if result.ci_lower <= true_mean <= result.ci_upper:
                n_covered += 1
        
        coverage = n_covered / n_trials
        
        # Should be close to 95%
        assert 0.85 <= coverage <= 1.0


class TestBCaCI:
    """Tests for BCa (bias-corrected and accelerated) confidence interval."""
    
    def test_basic(self):
        """Test basic BCa CI functionality."""
        np.random.seed(42)
        data = np.random.normal(0, 1, 100)
        
        result = bootstrap_ci(data, np.mean, method='bca', B=999, seed=42)
        
        assert result.ci_lower < result.estimate < result.ci_upper
        assert result.method == 'bca'
    
    def test_coverage_nominal(self):
        """Test that BCa CI has approximately nominal coverage.
        
        This is the key test: BCa should achieve ~95% coverage on a
        known distribution.
        """
        np.random.seed(42)
        n_trials = 200
        n_covered = 0
        true_mean = 5.0
        
        for i in range(n_trials):
            data = np.random.normal(true_mean, 1, 50)
            result = bootstrap_ci(data, np.mean, method='bca', B=999, seed=i)
            
            if result.ci_lower <= true_mean <= result.ci_upper:
                n_covered += 1
        
        coverage = n_covered / n_trials
        
        # BCa should have good coverage (within tolerance)
        assert 0.88 <= coverage <= 1.0, f"BCa coverage {coverage:.2f} outside acceptable range"
    
    def test_symmetric_data(self):
        """Test BCa on symmetric data (normal distribution)."""
        np.random.seed(42)
        data = np.random.normal(10, 2, 200)
        true_mean = 10.0
        
        result = bootstrap_ci(data, np.mean, method='bca', B=999, seed=42)
        
        # CI should contain true mean
        assert result.ci_lower <= true_mean <= result.ci_upper
        
        # CI should be approximately symmetric around sample mean
        dist_to_lower = result.estimate - result.ci_lower
        dist_to_upper = result.ci_upper - result.estimate
        
        assert dist_to_lower == pytest.approx(dist_to_upper, rel=0.2)
    
    def test_skewed_data(self):
        """Test BCa on skewed data (exponential distribution)."""
        np.random.seed(42)
        data = np.random.exponential(2, 200)
        true_mean = 2.0
        
        result = bootstrap_ci(data, np.mean, method='bca', B=999, seed=42)
        
        # CI should contain true mean
        assert result.ci_lower <= true_mean <= result.ci_upper


class TestBootstrapTCI:
    """Tests for bootstrap-t confidence interval."""
    
    def test_basic(self):
        """Test basic bootstrap-t CI functionality."""
        np.random.seed(42)
        data = np.random.normal(0, 1, 100)
        
        result = bootstrap_ci(data, np.mean, method='bootstrap_t', B=999, seed=42)
        
        assert result.ci_lower < result.estimate < result.ci_upper
        assert result.method == 'bootstrap_t'


class TestCIResult:
    """Tests for CIResult object."""
    
    def test_coverage_check(self):
        """Test coverage check method."""
        np.random.seed(42)
        data = np.random.normal(0, 1, 100)
        
        result = bootstrap_ci(data, np.mean, method='percentile', B=999, seed=42)
        
        # True mean (0) should be in CI
        assert result.coverage_check(0.0)
        
        # Extreme value should not be in CI
        assert not result.coverage_check(100.0)
    
    def test_ci_width(self):
        """Test CI width computation."""
        np.random.seed(42)
        data = np.random.normal(0, 1, 100)
        
        result = bootstrap_ci(data, np.mean, method='percentile', B=999, seed=42)
        
        width = result.ci_width()
        assert width > 0
        assert width == pytest.approx(result.ci_upper - result.ci_lower, rel=1e-10)
    
    def test_summary(self):
        """Test summary output."""
        np.random.seed(42)
        data = np.random.normal(0, 1, 100)
        
        result = bootstrap_ci(data, np.mean, method='bca', B=99, seed=42)
        summary = result.summary()
        
        assert "Estimate:" in summary
        assert "CI" in summary
        assert "Method:" in summary


class TestDifferentCILevels:
    """Tests for different confidence levels."""
    
    def test_90_percent(self):
        """Test 90% CI."""
        data = np.random.normal(0, 1, 100)
        
        result = bootstrap_ci(data, np.mean, method='percentile', ci_level=0.90, B=999, seed=42)
        
        assert result.ci_level == 0.90
    
    def test_99_percent(self):
        """Test 99% CI."""
        data = np.random.normal(0, 1, 100)
        
        result_90 = bootstrap_ci(data, np.mean, method='percentile', ci_level=0.90, B=999, seed=42)
        result_99 = bootstrap_ci(data, np.mean, method='percentile', ci_level=0.99, B=999, seed=42)
        
        # 99% CI should be wider than 90% CI
        assert result_99.ci_width() > result_90.ci_width()


class TestEdgeCases:
    """Tests for edge cases."""
    
    def test_different_statistics(self):
        """Test CIs for different statistics."""
        data = np.random.normal(0, 1, 100)
        
        # Median
        result = bootstrap_ci(data, np.median, method='percentile', B=999, seed=42)
        assert result.ci_lower < result.estimate < result.ci_upper
        
        # Standard deviation
        result = bootstrap_ci(data, lambda x: np.std(x, ddof=1), method='percentile', B=999, seed=42)
        assert result.ci_lower < result.estimate < result.ci_upper
    
    def test_invalid_method(self):
        """Test that invalid method raises error."""
        data = np.random.normal(0, 1, 100)
        
        with pytest.raises(ValueError):
            bootstrap_ci(data, np.mean, method='invalid', B=999, seed=42)
