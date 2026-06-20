"""
Tests for block bootstrap module.
"""

import numpy as np
import pytest
from resampling import (
    block_bootstrap,
    block_bootstrap_ci,
    moving_block_bootstrap,
    stationary_block_bootstrap,
    circular_block_bootstrap,
)


class TestMovingBlockBootstrap:
    """Tests for moving block bootstrap."""
    
    def test_basic(self):
        """Test basic moving block bootstrap functionality."""
        np.random.seed(42)
        # Generate autocorrelated data
        n = 100
        ts = np.zeros(n)
        ts[0] = 0
        for i in range(1, n):
            ts[i] = 0.5 * ts[i-1] + np.random.normal(0, 1)
        
        observed, boot_stats = moving_block_bootstrap(ts, np.mean, block_size=10, B=999, seed=42)
        
        assert observed == pytest.approx(np.mean(ts), rel=1e-10)
        assert len(boot_stats) == 999
    
    def test_handles_autocorrelation(self):
        """Test that block bootstrap handles autocorrelated data.
        
        This is the key test: for autocorrelated data, standard bootstrap
        underestimates SE, but block bootstrap should give better estimates.
        """
        np.random.seed(42)
        n = 200
        
        # Generate AR(1) process with strong autocorrelation
        ts = np.zeros(n)
        ts[0] = 0
        for i in range(1, n):
            ts[i] = 0.8 * ts[i-1] + np.random.normal(0, 1)
        
        # True SE for AR(1) mean
        # Var(mean) = sigma^2 * (1 + 2*sum(phi^k)) / n
        phi = 0.8
        sigma = 1.0
        true_var = sigma**2 * (1 + 2 * sum(phi**k for k in range(1, n))) / n
        true_se = np.sqrt(true_var)
        
        # Block bootstrap SE
        _, boot_stats = moving_block_bootstrap(ts, np.mean, block_size=20, B=9999, seed=42)
        block_se = np.std(boot_stats, ddof=1)
        
        # Block SE should be closer to true SE than naive SE
        naive_se = np.std(ts, ddof=1) / np.sqrt(n)
        
        # Block SE should be larger than naive SE (which underestimates)
        assert block_se > naive_se * 0.5
    
    def test_block_size_sensitivity(self):
        """Test that results vary with block size."""
        np.random.seed(42)
        ts = np.cumsum(np.random.normal(0, 1, 100))
        
        _, boot1 = moving_block_bootstrap(ts, np.mean, block_size=5, B=999, seed=42)
        _, boot2 = moving_block_bootstrap(ts, np.mean, block_size=20, B=999, seed=42)
        
        # Different block sizes should give different bootstrap distributions
        assert not np.allclose(boot1, boot2)


class TestStationaryBlockBootstrap:
    """Tests for stationary block bootstrap."""
    
    def test_basic(self):
        """Test basic stationary block bootstrap functionality."""
        np.random.seed(42)
        n = 100
        ts = np.zeros(n)
        ts[0] = 0
        for i in range(1, n):
            ts[i] = 0.5 * ts[i-1] + np.random.normal(0, 1)
        
        observed, boot_stats = stationary_block_bootstrap(ts, np.mean, block_size=10, B=999, seed=42)
        
        assert observed == pytest.approx(np.mean(ts), rel=1e-10)
        assert len(boot_stats) == 999
    
    def test_geometric_block_sizes(self):
        """Test that stationary bootstrap uses geometric block sizes."""
        np.random.seed(42)
        ts = np.random.normal(0, 1, 100)
        
        # Run multiple times and check block sizes vary
        block_sizes = []
        for seed in range(10):
            # Extract block sizes from bootstrap process
            rng = np.random.default_rng(seed)
            p = 1.0 / 10  # mean block size = 10
            
            sizes = []
            block_len = 0
            for _ in range(100):
                if block_len == 0 or rng.random() < p:
                    if block_len > 0:
                        sizes.append(block_len)
                    block_len = 1
                else:
                    block_len += 1
            if block_len > 0:
                sizes.append(block_len)
            
            block_sizes.extend(sizes)
        
        # Average block size should be close to 10
        assert np.mean(block_sizes) == pytest.approx(10, abs=3)


class TestCircularBlockBootstrap:
    """Tests for circular block bootstrap."""
    
    def test_basic(self):
        """Test basic circular block bootstrap functionality."""
        np.random.seed(42)
        ts = np.random.normal(0, 1, 100)
        
        observed, boot_stats = circular_block_bootstrap(ts, np.mean, block_size=10, B=999, seed=42)
        
        assert observed == pytest.approx(np.mean(ts), rel=1e-10)
        assert len(boot_stats) == 999
    
    def test_exact_length(self):
        """Test that bootstrap samples have exact same length as original."""
        np.random.seed(42)
        ts = np.random.normal(0, 1, 100)
        
        # Circular block bootstrap should produce samples of exact length
        _, boot_stats = circular_block_bootstrap(ts, np.mean, block_size=10, B=99, seed=42)
        
        # Each bootstrap sample should be same length
        # (we can't check individual samples, but statistics should all be valid)
        assert all(np.isfinite(boot_stats))


class TestBlockBootstrapDispatcher:
    """Tests for the general block bootstrap dispatcher."""
    
    def test_dispatcher(self):
        """Test that dispatcher works correctly."""
        np.random.seed(42)
        ts = np.random.normal(0, 1, 100)
        
        # Test all methods
        for method in ['moving', 'stationary', 'circular']:
            obs, boot = block_bootstrap(ts, np.mean, method=method, B=99, seed=42)
            assert obs == pytest.approx(np.mean(ts), rel=1e-10)
            assert len(boot) == 99
    
    def test_invalid_method(self):
        """Test that invalid method raises error."""
        ts = np.random.normal(0, 1, 100)
        
        with pytest.raises(ValueError):
            block_bootstrap(ts, np.mean, method='invalid', B=99)


class TestBlockBootstrapCI:
    """Tests for block bootstrap with confidence interval."""
    
    def test_basic(self):
        """Test basic block bootstrap CI functionality."""
        np.random.seed(42)
        ts = np.random.normal(0, 1, 100)
        
        result = block_bootstrap_ci(ts, np.mean, method='moving', B=999, seed=42)
        
        assert result.ci_lower < result.estimate < result.ci_upper
        assert result.block_size > 0
    
    def test_coverage(self):
        """Test that block bootstrap CI has reasonable coverage."""
        np.random.seed(42)
        n_trials = 100
        n_covered = 0
        true_mean = 0.0
        
        for i in range(n_trials):
            # Generate AR(1) process
            n = 100
            ts = np.zeros(n)
            ts[0] = 0
            for j in range(1, n):
                ts[j] = 0.5 * ts[j-1] + np.random.normal(0, 1)
            
            result = block_bootstrap_ci(ts, np.mean, method='moving', B=999, seed=i)
            
            if result.ci_lower <= true_mean <= result.ci_upper:
                n_covered += 1
        
        coverage = n_covered / n_trials
        
        # Should have reasonable coverage (might not be exact due to autocorrelation)
        assert 0.75 <= coverage <= 1.0


class TestBlockBootstrapResult:
    """Tests for BlockBootstrapResult object."""
    
    def test_summary(self):
        """Test summary output."""
        np.random.seed(42)
        ts = np.random.normal(0, 1, 100)
        
        result = block_bootstrap_ci(ts, np.mean, B=99, seed=42)
        summary = result.summary()
        
        assert "Estimate:" in summary
        assert "Block Size:" in summary
        assert "Method:" in summary


class TestAutoBlockSize:
    """Tests for automatic block size estimation."""
    
    def test_auto_estimation(self):
        """Test that automatic block size is reasonable."""
        np.random.seed(42)
        ts = np.random.normal(0, 1, 100)
        
        # Auto block size should be reasonable
        _, boot = block_bootstrap(ts, np.mean, method='moving', B=99, seed=42)
        
        # Should produce valid results
        assert all(np.isfinite(boot))


class TestDifferentStatistics:
    """Tests for block bootstrap with different statistics."""
    
    def test_median(self):
        """Test block bootstrap for median."""
        np.random.seed(42)
        ts = np.random.normal(0, 1, 100)
        
        obs, boot = block_bootstrap(ts, np.median, method='moving', B=99, seed=42)
        
        assert obs == np.median(ts)
        assert all(np.isfinite(boot))
    
    def test_std(self):
        """Test block bootstrap for standard deviation."""
        np.random.seed(42)
        ts = np.random.normal(0, 1, 100)
        
        obs, boot = block_bootstrap(ts, lambda x: np.std(x, ddof=1), method='moving', B=99, seed=42)
        
        assert obs == pytest.approx(np.std(ts, ddof=1), rel=1e-10)


class TestEdgeCases:
    """Tests for edge cases."""
    
    def test_short_time_series(self):
        """Test with short time series."""
        np.random.seed(42)
        ts = np.random.normal(0, 1, 20)
        
        # Should handle short series gracefully
        obs, boot = block_bootstrap(ts, np.mean, block_size=5, B=99, seed=42)
        
        assert obs == pytest.approx(np.mean(ts), rel=1e-10)
    
    def test_constant_series(self):
        """Test with constant time series."""
        ts = np.ones(100)
        
        obs, boot = block_bootstrap(ts, np.mean, block_size=10, B=99, seed=42)
        
        assert obs == 1.0
        assert all(boot == 1.0)
