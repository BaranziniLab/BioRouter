"""
Tests for the CLI subcommands.

Tests the sort, bench, and list subcommands, including input validation,
seed reproducibility, and unknown algorithm handling.
"""

import pytest
import random
from io import StringIO
from unittest.mock import patch

from sorts.cli import main, build_parser, validate_algorithm, validate_distribution, generate_array


class TestArgumentValidation:
    """Test input validation functions."""
    
    def test_validate_algorithm_valid(self):
        """Test validation accepts known algorithms."""
        for algo in ['bubble', 'insertion', 'selection', 'merge', 'quick',
                     'heap', 'shell', 'counting', 'radix']:
            assert validate_algorithm(algo) == algo
    
    def test_validate_algorithm_invalid(self):
        """Test validation rejects unknown algorithms."""
        with pytest.raises(Exception) as exc_info:
            validate_algorithm('bogus')
        assert 'unknown algorithm' in str(exc_info.value).lower()
        assert 'bogus' in str(exc_info.value)
    
    def test_validate_algorithm_invalid_shows_available(self):
        """Test that error message lists available algorithms."""
        with pytest.raises(Exception) as exc_info:
            validate_algorithm('xyz')
        msg = str(exc_info.value)
        assert 'bubble' in msg
        assert 'quick' in msg
    
    def test_validate_distribution_valid(self):
        """Test validation accepts known distributions."""
        for dist in ['random', 'sorted', 'reversed', 'few-unique']:
            assert validate_distribution(dist) == dist
    
    def test_validate_distribution_invalid(self):
        """Test validation rejects unknown distributions."""
        with pytest.raises(Exception) as exc_info:
            validate_distribution('gaussian')
        assert 'unknown distribution' in str(exc_info.value).lower()


class TestSeedReproducibility:
    """Test that --seed produces reproducible arrays."""
    
    def test_generate_array_random_with_seed(self):
        """Test that same seed produces same random array."""
        arr1 = generate_array('random', 20, seed=42)
        arr2 = generate_array('random', 20, seed=42)
        assert arr1 == arr2
    
    def test_generate_array_random_different_seeds(self):
        """Test that different seeds produce different arrays."""
        arr1 = generate_array('random', 20, seed=42)
        arr2 = generate_array('random', 20, seed=99)
        # Extremely unlikely to be equal with different seeds
        assert arr1 != arr2
    
    def test_generate_array_random_no_seed(self):
        """Test that no seed produces arrays (non-deterministic)."""
        arr = generate_array('random', 20, seed=None)
        assert len(arr) == 20
    
    def test_generate_array_few_unique_with_seed(self):
        """Test that few-unique distribution is reproducible with seed."""
        arr1 = generate_array('few-unique', 30, seed=123)
        arr2 = generate_array('few-unique', 30, seed=123)
        assert arr1 == arr2
    
    def test_generate_array_sorted_ignores_seed(self):
        """Test that sorted distribution is deterministic regardless of seed."""
        arr1 = generate_array('sorted', 10, seed=1)
        arr2 = generate_array('sorted', 10, seed=999)
        assert arr1 == arr2 == list(range(10))
    
    def test_generate_array_reversed_ignores_seed(self):
        """Test that reversed distribution is deterministic regardless of seed."""
        arr1 = generate_array('reversed', 10, seed=1)
        arr2 = generate_array('reversed', 10, seed=999)
        assert arr1 == arr2 == list(range(10, 0, -1))


class TestListSubcommand:
    """Test the list subcommand."""
    
    def test_list_algorithms_default(self, capsys):
        """Test listing algorithms (default)."""
        ret = main(['list'])
        assert ret == 0
        output = capsys.readouterr().out
        assert 'bubble' in output
        assert 'quick' in output
        assert 'merge' in output
        assert 'radix' in output
    
    def test_list_algorithms_explicit(self, capsys):
        """Test listing algorithms explicitly."""
        ret = main(['list', 'algorithms'])
        assert ret == 0
        output = capsys.readouterr().out
        assert 'bubble' in output
        assert 'heap' in output
    
    def test_list_algorithms_with_info(self, capsys):
        """Test listing algorithms with detailed info."""
        ret = main(['list', 'algorithms', '--info'])
        assert ret == 0
        output = capsys.readouterr().out
        assert 'Time Complexity' in output
        assert 'Space Complexity' in output
        assert 'Stable' in output
    
    def test_list_distributions(self, capsys):
        """Test listing distributions."""
        ret = main(['list', 'distributions'])
        assert ret == 0
        output = capsys.readouterr().out
        assert 'random' in output
        assert 'sorted' in output
        assert 'reversed' in output
        assert 'few-unique' in output


class TestSortSubcommand:
    """Test the sort subcommand."""
    
    def test_sort_basic(self, capsys):
        """Test basic sort subcommand."""
        ret = main(['sort', 'bubble', '-n', '10', '--speed', '0'])
        assert ret == 0
        output = capsys.readouterr().out
        assert 'bubble' in output.lower()
    
    def test_sort_with_seed(self, capsys):
        """Test sort subcommand with seed."""
        ret = main(['sort', 'quick', '-n', '15', '--seed', '42', '--speed', '0'])
        assert ret == 0
        output = capsys.readouterr().out
        assert 'seed=42' in output
    
    def test_sort_with_distribution(self, capsys):
        """Test sort subcommand with distribution."""
        ret = main(['sort', 'merge', '-n', '10', '-d', 'sorted', '--speed', '0'])
        assert ret == 0
        output = capsys.readouterr().out
        assert 'sorted' in output
    
    def test_sort_unknown_algorithm(self, capsys):
        """Test sort with unknown algorithm name shows helpful error."""
        with pytest.raises(SystemExit) as exc_info:
            main(['sort', 'bogus'])
        assert exc_info.value.code == 2
        # Error is printed to stderr by argparse
        stderr = capsys.readouterr().err
        assert 'bogus' in stderr
        assert 'Available algorithms' in stderr
    
    def test_sort_all_algorithms(self, capsys):
        """Test that all algorithms can be invoked via sort subcommand."""
        algorithms = ['bubble', 'insertion', 'selection', 'merge', 'quick',
                      'heap', 'shell', 'counting', 'radix']
        for algo in algorithms:
            ret = main(['sort', algo, '-n', '5', '--speed', '0'])
            assert ret == 0, f"Algorithm {algo} failed"


class TestBenchSubcommand:
    """Test the bench subcommand."""
    
    def test_bench_basic(self, capsys):
        """Test basic bench subcommand with small sizes."""
        ret = main(['bench', '-a', 'bubble', 'insertion', '--sizes', '20', 
                     '--trials', '1', '--distributions', 'random'])
        assert ret == 0
        output = capsys.readouterr().out
        assert 'BENCHMARK RESULTS' in output
        assert 'bubble' in output
        assert 'insertion' in output
    
    def test_bench_with_seed(self, capsys):
        """Test bench subcommand with seed for reproducibility."""
        ret = main(['bench', '-a', 'bubble', '--sizes', '20', 
                     '--trials', '1', '--seed', '42'])
        assert ret == 0
        output = capsys.readouterr().out
        assert 'seed: 42' in output
    
    def test_bench_unknown_algorithm(self):
        """Test bench with unknown algorithm name."""
        with pytest.raises(SystemExit) as exc_info:
            main(['bench', '-a', 'bogus'])
        assert exc_info.value.code == 2


class TestNoSubcommand:
    """Test behavior when no subcommand is given."""
    
    def test_no_subcommand_shows_help(self, capsys):
        """Test that no subcommand prints help and returns 0."""
        ret = main([])
        assert ret == 0
        output = capsys.readouterr().out
        # Help text should mention the subcommands
        assert 'sort' in output.lower()
        assert 'bench' in output.lower()
        assert 'list' in output.lower()


class TestBuildParser:
    """Test parser construction."""
    
    def test_parser_has_subcommands(self):
        """Test that parser has the expected subcommands."""
        parser = build_parser()
        # Parse known subcommands without error
        parser.parse_args(['list'])
        parser.parse_args(['sort', 'bubble'])
        parser.parse_args(['bench'])
    
    def test_parser_sort_defaults(self):
        """Test sort subcommand default values."""
        parser = build_parser()
        args = parser.parse_args(['sort', 'bubble'])
        assert args.algorithm == 'bubble'
        assert args.size == 20
        assert args.distribution == 'random'
        assert args.speed == 0.1
        assert args.seed is None
    
    def test_parser_sort_custom(self):
        """Test sort subcommand with custom values."""
        parser = build_parser()
        args = parser.parse_args(['sort', 'quick', '-n', '50', '-d', 'reversed',
                                   '-s', '0.5', '--seed', '123'])
        assert args.algorithm == 'quick'
        assert args.size == 50
        assert args.distribution == 'reversed'
        assert args.speed == 0.5
        assert args.seed == 123
    
    def test_parser_bench_defaults(self):
        """Test bench subcommand default values."""
        parser = build_parser()
        args = parser.parse_args(['bench'])
        assert args.sizes == [100, 500, 1000]
        assert args.trials == 3
        assert args.seed is None
        assert len(args.algorithms) == 9
        assert len(args.distributions) == 4


if __name__ == '__main__':
    pytest.main([__file__, '-v'])
