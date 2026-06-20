"""
Tests for bootstrap module.
"""

import numpy as np
import pytest
from resampling import (
    bootstrap,
    bootstrap_analysis,
    bootstrap_se,
    bootstrap_bias,
    nonparametric_bootstrap,
    parametric_bootstrap,
    smoothed_bootstrap,
)


class TestNonparametricBootstrap:
    """Tests for nonparametric (case) bootstrap."""
    
    def test_basic(self):
        """Test basic bootstrap functionality."""
        np.random.seed(42)
        data = np.random.normal(0, 1, 100)
        
        observed, boot_stats = nonparametric_bootstrap(data, np.mean, B=999, seed=42)
        
        assert observed == pytest.approx(np.mean(data), rel=1e-10)
        assert len(boot_stats) == 999
    
    def test_se_reproducibility(self):
        """Test that bootstrap SE is reproducible with same seed."""
        data = np.random.normal(0, 1, 100)
        
        _, boot1 = nonparametric_bootstrap(data, np.mean, B=999, seed=42)
        _, boot2 = nonparametric_bootstrap(data, np.mean, B=999, seed=42)
        
        np.testing.assert_array_equal(boot1, boot2)
    
    def test_se_different_seeds(self):
        """Test that different seeds give different results."""
        data = np.random.normal(0, 1, 100)
        
        _, boot1 = nonparametric_bootstrap(data, np.mean, B=999, seed=42)
        _, boot2 = nonparametric_bootstrap(data, np.mean, B=999, seed=123)
        
        assert not np.allclose(boot1, boot2)
    
    def test_se_converges_to_analytic(self):
        """Test that bootstrap SE converges to analytic SE for mean."""
        np.random.seed(42)
        data = np.random.normal(0, 1, 1000)
        
        # Analytic SE for mean = std / sqrt(n)
        analytic_se = np.std(data, ddof=1) / np.sqrt(len(data))
        
        # Bootstrap SE with large B
        se = bootstrap_se(data, np.mean, B=9999, seed=42)
        
        # Should be within 10% of analytic SE
        assert se == pytest.approx(analytic_se, rel=0.10)
    
    def test_different_statistics(self):
        """Test bootstrap with different statistics."""
        data = np.random.normal(0, 1, 100)
        
        # Test median
        observed, boot = nonparametric_bootstrap(data, np.median, B=99, seed=42)
        assert observed == np.median(data)
        assert len(boot) == 99
        
        # Test std
        observed, boot = nonparametric_bootstrap(data, lambda x: np.std(x, ddof=1), B=99, seed=42)
        assert observed == pytest.approx(np.std(data, ddof=1), rel=1e-10)


class TestParametricBootstrap:
    """Tests for parametric bootstrap."""
    
    def test_normal(self):
        """Test parametric bootstrap with normal model."""
        np.random.seed(42)
        data = np.random.normal(5, 2, 100)
        
        observed, boot_stats = parametric_bootstrap(
            data, np.mean, B=999, model='normal', seed=42
        )
        
        assert observed == pytest.approx(np.mean(data), rel=1e-10)
        assert len(boot_stats) == 999
    
    def test_exponential(self):
        """Test parametric bootstrap with exponential model."""
        np.random.seed(42)
        data = np.random.exponential(2, 100)
        
        observed, boot_stats = parametric_bootstrap(
            data, np.mean, B=999, model='exponential', seed=42
        )
        
        assert observed == pytest.approx(np.mean(data), rel=1e-10)
    
    def test_poisson(self):
        """Test parametric bootstrap with Poisson model."""
        np.random.seed(42)
        data = np.random.poisson(5, 100).astype(float)
        
        observed, boot_stats = parametric_bootstrap(
            data, np.mean, B=999, model='poisson', seed=42
        )
        
        assert observed == pytest.approx(np.mean(data), rel=1e-10)
    
    def test_custom_params(self):
        """Test parametric bootstrap with custom parameters."""
        np.random.seed(42)
        data = np.random.normal(0, 1, 100)
        
        # Use different parameters than fitted from data
        observed, boot_stats = parametric_bootstrap(
            data, np.mean, B=999, model='normal', mu=10, sigma=5, seed=42
        )
        
        # Bootstrap mean should be around custom mu=10
        assert np.mean(boot_stats) == pytest.approx(10, abs=0.5)


class TestSmoothedBootstrap:
    """Tests for smoothed bootstrap."""
    
    def test_basic(self):
        """Test basic smoothed bootstrap functionality."""
        np.random.seed(42)
        data = np.random.normal(0, 1, 100)
        
        observed, boot_stats = smoothed_bootstrap(data, np.mean, B=999, seed=42)
        
        assert observed == pytest.approx(np.mean(data), rel=1e-10)
        assert len(boot_stats) == 999
    
    def test_custom_bandwidth(self):
        """Test smoothed bootstrap with custom bandwidth."""
        np.random.seed(42)
        data = np.random.normal(0, 1, 100)
        
        observed, boot1 = smoothed_bootstrap(data, np.mean, B=999, bandwidth=0.5, seed=42)
        observed, boot2 = smoothed_bootstrap(data, np.mean, B=999, bandwidth=2.0, seed=42)
        
        # Different bandwidths should give different results
        assert not np.allclose(boot1, boot2)


class TestBootstrapDispatcher:
    """Tests for the general bootstrap dispatcher."""
    
    def test_dispatcher(self):
        """Test that dispatcher works correctly."""
        data = np.random.normal(0, 1, 100)
        
        # Nonparametric
        obs1, boot1 = bootstrap(data, np.mean, method='nonparametric', B=99, seed=42)
        
        # Parametric
        obs2, boot2 = bootstrap(data, np.mean, method='parametric', B=99, seed=42)
        
        # Smoothed
        obs3, boot3 = bootstrap(data, np.mean, method='smoothed', B=99, seed=42)
        
        assert obs1 == obs2 == obs3
        assert len(boot1) == len(boot2) == len(boot3) == 99
    
    def test_invalid_method(self):
        """Test that invalid method raises error."""
        data = np.random.normal(0, 1, 100)
        
        with pytest.raises(ValueError):
            bootstrap(data, np.mean, method='invalid', B=99)


class TestBootstrapSE:
    """Tests for bootstrap standard error."""
    
    def test_basic(self):
        """Test bootstrap SE computation."""
        data = np.random.normal(0, 1, 100)
        
        se = bootstrap_se(data, np.mean, B=999, seed=42)
        
        # SE should be positive
        assert se > 0
        
        # SE should be close to analytic SE
        analytic_se = np.std(data, ddof=1) / np.sqrt(len(data))
        assert se == pytest.approx(analytic_se, rel=0.15)


class TestBootstrapBias:
    """Tests for bootstrap bias estimation."""
    
    def test_unbiased_statistic(self):
        """Test bias estimation for unbiased statistic (mean)."""
        np.random.seed(42)
        data = np.random.normal(0, 1, 100)
        
        observed, bias = bootstrap_bias(data, np.mean, B=9999, seed=42)
        
        # Mean is unbiased, so bias should be close to 0
        assert bias == pytest.approx(0, abs=0.1)
    
    def test_biased_statistic(self):
        """Test bias estimation for biased statistic."""
        np.random.seed(42)
        data = np.random.normal(0, 1, 100)
        
        # Define a biased estimator
        def biased_estimator(x):
            return np.mean(x) + 1.0  # Always biased by +1
        
        observed, bias = bootstrap_bias(data, biased_estimator, B=9999, seed=42)
        
        # Bootstrap bias = mean(T*) - T_obs
        # For this estimator, mean(T*) should be close to T_obs
        # So bias should be close to 0, not +1
        # The +1 bias is between T and true_value, not between T* and T
        assert bias == pytest.approx(0, abs=0.1)


class TestBootstrapAnalysis:
    """Tests for complete bootstrap analysis."""
    
    def test_result_object(self):
        """Test that analysis returns proper result object."""
        data = np.random.normal(0, 1, 100)
        
        result = bootstrap_analysis(data, np.mean, B=999, seed=42)
        
        assert result.estimate == pytest.approx(np.mean(data), rel=1e-10)
        assert result.std_error > 0
        assert result.n_resamples == 999
        assert result.method == 'nonparametric'
    
    def test_summary(self):
        """Test summary output."""
        data = np.random.normal(0, 1, 100)
        
        result = bootstrap_analysis(data, np.mean, B=99, seed=42)
        summary = result.summary()
        
        assert "Estimate:" in summary
        assert "Std Error:" in summary
        assert "Resamples:" in summary
    
    def test_convergence_data(self):
        """Test convergence plot data generation."""
        data = np.random.normal(0, 1, 100)
        
        result = bootstrap_analysis(data, np.mean, B=999, seed=42)
        conv = result.convergence_plot_data()
        
        assert 'k_values' in conv
        assert 'se_values' in conv
        assert len(conv['k_values']) > 0
        assert len(conv['k_values']) == len(conv['se_values'])


class TestEdgeCases:
    """Tests for edge cases and error handling."""
    
    def test_empty_data(self):
        """Test that empty data raises error."""
        with pytest.raises(ValueError):
            nonparametric_bootstrap([], np.mean, B=99)
    
    def test_single_observation(self):
        """Test with single observation."""
        data = np.array([5.0])
        
        # Should work but bootstrap will always return the same value
        observed, boot_stats = nonparametric_bootstrap(data, np.mean, B=99, seed=42)
        
        assert observed == 5.0
        assert np.all(boot_stats == 5.0)
    
    def test_non_callable_stat(self):
        """Test that non-callable statistic raises error."""
        data = np.random.normal(0, 1, 100)
        
        with pytest.raises(ValueError):
            nonparametric_bootstrap(data, "not_a_function", B=99)
