"""
Tests for jackknife module.
"""

import numpy as np
import pytest
from resampling import (
    jackknife,
    jackknife_variance,
    jackknife_bias,
    jackknife_ci,
)


class TestJackknifeLOO:
    """Tests for leave-one-out jackknife."""
    
    def test_basic(self):
        """Test basic LOO jackknife functionality."""
        np.random.seed(42)
        data = np.random.normal(0, 1, 100)
        
        result = jackknife(data, np.mean, method='loo')
        
        assert result.estimate == pytest.approx(np.mean(data), rel=1e-10)
        assert result.method == 'loo'
        assert result.n_resamples == 100
    
    def test_unbiased_statistic(self):
        """Test jackknife on unbiased statistic (mean)."""
        np.random.seed(42)
        data = np.random.normal(0, 1, 100)
        
        result = jackknife(data, np.mean, method='loo')
        
        # Mean is unbiased, so jackknife bias should be close to 0
        assert result.bias == pytest.approx(0, abs=0.01)
    
    def test_biased_statistic(self):
        """Test jackknife on biased statistic."""
        np.random.seed(42)
        data = np.random.normal(0, 1, 100)
        
        # Define a biased estimator: variance with ddof=0
        def biased_variance(x):
            return np.var(x)  # Biased (ddof=0)
        
        result = jackknife(data, biased_variance, method='loo')
        
        # True variance (ddof=1)
        true_var = np.var(data, ddof=1)
        
        # Bias should be approximately true_var - biased_var
        biased_var = biased_variance(data)
        expected_bias = biased_var - true_var
        
        assert result.bias == pytest.approx(expected_bias, rel=0.2)
    
    def test_variance_estimation(self):
        """Test jackknife variance estimation."""
        np.random.seed(42)
        data = np.random.normal(0, 1, 100)
        
        result = jackknife(data, np.mean, method='loo')
        
        # Jackknife variance for mean should be close to true variance
        true_var = np.var(data, ddof=1) / len(data)
        
        assert result.variance == pytest.approx(true_var, rel=0.2)
    
    def test_bias_corrected(self):
        """Test bias-corrected estimate."""
        np.random.seed(42)
        data = np.random.normal(0, 1, 100)
        
        def biased_estimator(x):
            return np.mean(x) + 2.0
        
        result = jackknife(data, biased_estimator, method='loo')
        
        # The jackknife estimates bias = (n-1)(T_jack - T_obs)
        # For this estimator, T_jack ≈ T_obs, so bias ≈ 0
        # The constant +2 affects T but not the jackknife bias estimate
        # because the bias is constant across all jackknife samples
        assert result.bias == pytest.approx(0, abs=0.5)
        
        # bias_corrected = observed - bias ≈ observed
        assert result.bias_corrected == pytest.approx(result.estimate, abs=0.5)


class TestJackknifeDeleteD:
    """Tests for delete-d jackknife."""
    
    def test_basic(self):
        """Test basic delete-d jackknife functionality."""
        np.random.seed(42)
        data = np.random.normal(0, 1, 100)
        
        result = jackknife(data, np.mean, method='delete-d')
        
        assert result.estimate == pytest.approx(np.mean(data), rel=1e-10)
        assert result.method == 'delete-d'


class TestJackknifeVariance:
    """Tests for jackknife variance function."""
    
    def test_basic(self):
        """Test jackknife variance computation."""
        data = np.random.normal(0, 1, 100)
        
        var = jackknife_variance(data, np.mean)
        
        # Variance should be positive
        assert var > 0
        
        # Should be close to true variance of mean
        true_var = np.var(data, ddof=1) / len(data)
        assert var == pytest.approx(true_var, rel=0.2)


class TestJackknifeBias:
    """Tests for jackknife bias function."""
    
    def test_unbiased(self):
        """Test jackknife bias on unbiased statistic."""
        data = np.random.normal(0, 1, 100)
        
        bias = jackknife_bias(data, np.mean)
        
        # Mean is unbiased
        assert bias == pytest.approx(0, abs=0.01)
    
    def test_biased(self):
        """Test jackknife bias on biased statistic."""
        np.random.seed(42)
        data = np.random.normal(0, 1, 100)
        
        # Biased estimator
        def biased(x):
            return np.mean(x) + 1.0
        
        bias = jackknife_bias(data, biased)
        
        # The jackknife bias estimate is (n-1)(T_jack - T_obs)
        # For a constant bias like +1, this should be close to 0
        # because the bias is constant across all jackknife samples
        assert bias == pytest.approx(0, abs=0.1)


class TestJackknifeCI:
    """Tests for jackknife confidence interval."""
    
    def test_basic(self):
        """Test basic jackknife CI functionality."""
        data = np.random.normal(0, 1, 100)
        
        lower, upper = jackknife_ci(data, np.mean, ci_level=0.95)
        
        assert lower < np.mean(data) < upper
    
    def test_coverage(self):
        """Test that jackknife CI has reasonable coverage."""
        np.random.seed(42)
        n_trials = 100
        n_covered = 0
        true_mean = 5.0
        
        for i in range(n_trials):
            data = np.random.normal(true_mean, 1, 50)
            lower, upper = jackknife_ci(data, np.mean, ci_level=0.95)
            
            if lower <= true_mean <= upper:
                n_covered += 1
        
        coverage = n_covered / n_trials
        
        # Should be close to 95% (might be lower for small samples)
        assert 0.80 <= coverage <= 1.0


class TestJackknifeResult:
    """Tests for JackknifeResult object."""
    
    def test_summary(self):
        """Test summary output."""
        data = np.random.normal(0, 1, 100)
        
        result = jackknife(data, np.mean)
        summary = result.summary()
        
        assert "Estimate:" in summary
        assert "Bias:" in summary
        assert "Std Error:" in summary
        assert "Method:" in summary
    
    def test_repr(self):
        """Test repr output."""
        data = np.random.normal(0, 1, 100)
        
        result = jackknife(data, np.mean)
        repr_str = repr(result)
        
        assert "Jackknife Result" in repr_str


class TestDifferentStatistics:
    """Tests for jackknife with different statistics."""
    
    def test_median(self):
        """Test jackknife for median."""
        data = np.random.normal(0, 1, 100)
        
        result = jackknife(data, np.median)
        
        assert result.estimate == np.median(data)
        assert result.std_error > 0
    
    def test_std(self):
        """Test jackknife for standard deviation."""
        data = np.random.normal(0, 1, 100)
        
        result = jackknife(data, lambda x: np.std(x, ddof=1))
        
        assert result.estimate == pytest.approx(np.std(data, ddof=1), rel=1e-10)
        assert result.std_error > 0


class TestEdgeCases:
    """Tests for edge cases."""
    
    def test_small_sample(self):
        """Test jackknife with small sample."""
        data = np.array([1.0, 2.0, 3.0, 4.0, 5.0])
        
        result = jackknife(data, np.mean)
        
        assert result.estimate == 3.0
        assert result.n_resamples == 5
    
    def test_constant_data(self):
        """Test jackknife with constant data."""
        data = np.ones(10)
        
        result = jackknife(data, np.mean)
        
        assert result.estimate == 1.0
        assert result.bias == 0.0
        assert result.variance == 0.0
