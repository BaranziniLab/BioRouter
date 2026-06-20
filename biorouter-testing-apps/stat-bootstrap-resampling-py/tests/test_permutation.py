"""
Tests for permutation tests module.
"""

import numpy as np
import pytest
from resampling import (
    permutation_test,
    two_sample_test,
    paired_test,
    correlation_test,
)


class TestTwoSampleTest:
    """Tests for two-sample permutation test."""
    
    def test_basic(self):
        """Test basic two-sample test functionality."""
        np.random.seed(42)
        group1 = np.random.normal(0, 1, 50)
        group2 = np.random.normal(0, 1, 50)
        
        result = two_sample_test(group1, group2, B=999, seed=42)
        
        assert 0 <= result.p_value <= 1
        assert result.n_permutations == 1000
    
    def test_type_i_error(self):
        """Test that type-I error is approximately alpha under null.
        
        This is the key test: under H0 (same distribution), the permutation
        test should reject at rate approximately equal to alpha.
        """
        np.random.seed(42)
        n_trials = 200
        n_rejected = 0
        alpha = 0.05
        
        for i in range(n_trials):
            # Generate from same distribution (H0 true)
            group1 = np.random.normal(0, 1, 50)
            group2 = np.random.normal(0, 1, 50)
            
            result = two_sample_test(group1, group2, B=999, seed=i)
            
            if result.p_value < alpha:
                n_rejected += 1
        
        type_i_rate = n_rejected / n_trials
        
        # Type-I error should be close to alpha
        assert 0.02 <= type_i_rate <= 0.12, f"Type-I error {type_i_rate:.2f} outside acceptable range"
    
    def test_power(self):
        """Test that power is high under strong effect.
        
        When there's a large difference between groups, the test should
        have high power to detect it.
        """
        np.random.seed(42)
        n_trials = 100
        n_rejected = 0
        alpha = 0.05
        
        for i in range(n_trials):
            # Generate with large difference (effect size ~1.0)
            group1 = np.random.normal(0, 1, 50)
            group2 = np.random.normal(2, 1, 50)  # Mean shift of 2
            
            result = two_sample_test(group1, group2, B=999, seed=i)
            
            if result.p_value < alpha:
                n_rejected += 1
        
        power = n_rejected / n_trials
        
        # Power should be very high for large effect
        assert power >= 0.90, f"Power {power:.2f} too low for strong effect"
    
    def test_alternatives(self):
        """Test different alternative hypotheses."""
        np.random.seed(42)
        group1 = np.random.normal(0, 1, 50)
        group2 = np.random.normal(1, 1, 50)
        
        # Two-sided
        result = two_sample_test(group1, group2, alternative='two-sided', B=999, seed=42)
        assert result.alternative == 'two-sided'
        
        # Greater (group1 > group2)
        result = two_sample_test(group1, group2, alternative='greater', B=999, seed=42)
        assert result.alternative == 'greater'
        
        # Less (group1 < group2)
        result = two_sample_test(group1, group2, alternative='less', B=999, seed=42)
        assert result.alternative == 'less'


class TestPairedTest:
    """Tests for paired permutation test."""
    
    def test_basic(self):
        """Test basic paired test functionality."""
        np.random.seed(42)
        sample1 = np.random.normal(0, 1, 50)
        sample2 = np.random.normal(0.5, 1, 50)
        
        result = paired_test(sample1, sample2, B=999, seed=42)
        
        assert 0 <= result.p_value <= 1
    
    def test_type_i_error(self):
        """Test type-I error for paired test."""
        np.random.seed(42)
        n_trials = 200
        n_rejected = 0
        alpha = 0.05
        
        for i in range(n_trials):
            # Generate paired data from same distribution
            base = np.random.normal(0, 1, 50)
            sample1 = base + np.random.normal(0, 0.1, 50)
            sample2 = base + np.random.normal(0, 0.1, 50)
            
            result = paired_test(sample1, sample2, B=999, seed=i)
            
            if result.p_value < alpha:
                n_rejected += 1
        
        type_i_rate = n_rejected / n_trials
        
        # Type-I error should be close to alpha
        assert 0.02 <= type_i_rate <= 0.12
    
    def test_power(self):
        """Test power for paired test."""
        np.random.seed(42)
        n_trials = 100
        n_rejected = 0
        alpha = 0.05
        
        for i in range(n_trials):
            # Generate paired data with systematic difference
            base = np.random.normal(0, 1, 50)
            sample1 = base
            sample2 = base + 1.0  # Systematic difference
            
            result = paired_test(sample1, sample2, B=999, seed=i)
            
            if result.p_value < alpha:
                n_rejected += 1
        
        power = n_rejected / n_trials
        
        # Power should be very high
        assert power >= 0.90


class TestCorrelationTest:
    """Tests for correlation permutation test."""
    
    def test_basic(self):
        """Test basic correlation test functionality."""
        np.random.seed(42)
        x = np.random.normal(0, 1, 50)
        y = 0.7 * x + np.random.normal(0, 0.5, 50)
        
        result = correlation_test(x, y, B=999, seed=42)
        
        assert 0 <= result.p_value <= 1
        assert result.test_statistic == pytest.approx(np.corrcoef(x, y)[0, 1], rel=1e-10)
    
    def test_type_i_error(self):
        """Test type-I error for correlation test."""
        np.random.seed(42)
        n_trials = 200
        n_rejected = 0
        alpha = 0.05
        
        for i in range(n_trials):
            # Generate independent variables (H0: no correlation)
            x = np.random.normal(0, 1, 50)
            y = np.random.normal(0, 1, 50)
            
            result = correlation_test(x, y, B=999, seed=i)
            
            if result.p_value < alpha:
                n_rejected += 1
        
        type_i_rate = n_rejected / n_trials
        
        # Type-I error should be close to alpha
        assert 0.02 <= type_i_rate <= 0.12
    
    def test_power(self):
        """Test power for correlation test."""
        np.random.seed(42)
        n_trials = 100
        n_rejected = 0
        alpha = 0.05
        
        for i in range(n_trials):
            # Generate strongly correlated variables
            x = np.random.normal(0, 1, 100)
            y = 0.9 * x + np.random.normal(0, 0.3, 100)
            
            result = correlation_test(x, y, B=999, seed=i)
            
            if result.p_value < alpha:
                n_rejected += 1
        
        power = n_rejected / n_trials
        
        # Power should be very high for strong correlation
        assert power >= 0.90


class TestPermutationTest:
    """Tests for general permutation test."""
    
    def test_basic(self):
        """Test basic permutation test functionality."""
        np.random.seed(42)
        sample1 = np.random.normal(0, 1, 50)
        sample2 = np.random.normal(1, 1, 50)
        
        result = permutation_test(sample1, sample2, B=999, seed=42)
        
        assert 0 <= result.p_value <= 1
    
    def test_custom_statistic(self):
        """Test permutation test with custom statistic."""
        np.random.seed(42)
        sample1 = np.random.normal(0, 1, 50)
        sample2 = np.random.normal(1, 1, 50)
        
        # Use difference in medians
        def diff_medians(x):
            n1 = len(sample1)
            return np.median(x[:n1]) - np.median(x[n1:])
        
        result = permutation_test(sample1, sample2, stat=diff_medians, B=999, seed=42)
        
        assert 0 <= result.p_value <= 1


class TestPermutationResult:
    """Tests for PermutationResult object."""
    
    def test_summary(self):
        """Test summary output."""
        np.random.seed(42)
        group1 = np.random.normal(0, 1, 50)
        group2 = np.random.normal(1, 1, 50)
        
        result = two_sample_test(group1, group2, B=99, seed=42)
        summary = result.summary()
        
        assert "Test Statistic:" in summary
        assert "P-value:" in summary
        assert "Method:" in summary
    
    def test_is_significant(self):
        """Test is_significant method."""
        np.random.seed(42)
        group1 = np.random.normal(0, 1, 50)
        group2 = np.random.normal(10, 1, 50)  # Large difference
        
        result = two_sample_test(group1, group2, B=999, seed=42)
        
        # Should be significant at alpha=0.05
        assert result.is_significant(0.05)


class TestExactTest:
    """Tests for exact permutation test."""
    
    def test_exact_small_sample(self):
        """Test exact permutation test for small samples."""
        np.random.seed(42)
        sample1 = np.array([1.0, 2.0, 3.0])
        sample2 = np.array([4.0, 5.0, 6.0])
        
        result = permutation_test(sample1, sample2, exact=True, B=999, seed=42)
        
        assert 0 <= result.p_value <= 1
        assert result.method == 'exact'


class TestEdgeCases:
    """Tests for edge cases."""
    
    def test_equal_samples(self):
        """Test with identical samples."""
        np.random.seed(42)
        data = np.random.normal(0, 1, 50)
        
        result = two_sample_test(data, data, B=999, seed=42)
        
        # P-value should be high (can't reject H0)
        assert result.p_value > 0.1
    
    def test_different_sample_sizes(self):
        """Test with different sample sizes."""
        np.random.seed(42)
        group1 = np.random.normal(0, 1, 30)
        group2 = np.random.normal(1, 1, 50)
        
        result = two_sample_test(group1, group2, B=999, seed=42)
        
        assert 0 <= result.p_value <= 1
