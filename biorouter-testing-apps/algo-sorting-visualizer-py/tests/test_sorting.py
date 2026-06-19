"""
Test suite for sorting algorithms.

Tests correctness, stability, and edge cases for all sorting algorithms.
"""

import pytest
import random
from typing import List, Any

from sorts import (
    bubble_sort, insertion_sort, selection_sort, merge_sort, quick_sort,
    heap_sort, shell_sort, counting_sort, radix_sort
)
from sorts.base import SortState


# List of all sorting algorithms
ALL_SORTS = [
    bubble_sort, insertion_sort, selection_sort, merge_sort, quick_sort,
    heap_sort, shell_sort, counting_sort, radix_sort
]

# Algorithms that support negative numbers
NEGATIVE_SUPPORT = [bubble_sort, insertion_sort, selection_sort, merge_sort, 
                   quick_sort, heap_sort, shell_sort]

# Algorithms that support general comparable types (not just integers)
GENERAL_SORTS = [bubble_sort, insertion_sort, selection_sort, merge_sort, 
                quick_sort, heap_sort, shell_sort]

# Stable sorting algorithms
STABLE_SORTS = [bubble_sort, insertion_sort, merge_sort, counting_sort, radix_sort]

# Stable sorting algorithms that support general comparable types
STABLE_GENERAL_SORTS = [bubble_sort, insertion_sort, merge_sort]


def get_sorted_result(sort_func, data: List[Any]) -> List[Any]:
    """
    Run a sorting algorithm and return the final sorted array.
    
    Args:
        sort_func: Sorting function that yields SortState objects
        data: List of elements to sort
        
    Returns:
        Sorted list
    """
    arr = data.copy()
    last_state = None
    for state in sort_func(arr):
        last_state = state
    return last_state.array if last_state else arr


class TestSortingCorrectness:
    """Test that all sorting algorithms produce correct results."""
    
    @pytest.mark.parametrize("sort_func", ALL_SORTS)
    def test_empty_array(self, sort_func):
        """Test sorting an empty array."""
        result = get_sorted_result(sort_func, [])
        assert result == []
    
    @pytest.mark.parametrize("sort_func", ALL_SORTS)
    def test_single_element(self, sort_func):
        """Test sorting a single element."""
        result = get_sorted_result(sort_func, [42])
        assert result == [42]
    
    @pytest.mark.parametrize("sort_func", ALL_SORTS)
    def test_two_elements_sorted(self, sort_func):
        """Test sorting two elements that are already sorted."""
        result = get_sorted_result(sort_func, [1, 2])
        assert result == [1, 2]
    
    @pytest.mark.parametrize("sort_func", ALL_SORTS)
    def test_two_elements_unsorted(self, sort_func):
        """Test sorting two elements that are unsorted."""
        result = get_sorted_result(sort_func, [2, 1])
        assert result == [1, 2]
    
    @pytest.mark.parametrize("sort_func", ALL_SORTS)
    def test_random_array(self, sort_func):
        """Test sorting a random array."""
        random.seed(42)  # For reproducibility
        data = [random.randint(0, 100) for _ in range(20)]
        expected = sorted(data)
        result = get_sorted_result(sort_func, data)
        assert result == expected
    
    @pytest.mark.parametrize("sort_func", ALL_SORTS)
    def test_sorted_array(self, sort_func):
        """Test sorting an already sorted array."""
        data = list(range(10))
        result = get_sorted_result(sort_func, data)
        assert result == data
    
    @pytest.mark.parametrize("sort_func", ALL_SORTS)
    def test_reverse_sorted_array(self, sort_func):
        """Test sorting a reverse sorted array."""
        data = list(range(10, 0, -1))
        expected = list(range(1, 11))
        result = get_sorted_result(sort_func, data)
        assert result == expected
    
    @pytest.mark.parametrize("sort_func", ALL_SORTS)
    def test_duplicates(self, sort_func):
        """Test sorting an array with duplicates."""
        data = [3, 1, 4, 1, 5, 9, 2, 6, 5, 3, 5]
        expected = sorted(data)
        result = get_sorted_result(sort_func, data)
        assert result == expected
    
    @pytest.mark.parametrize("sort_func", ALL_SORTS)
    def test_all_same_elements(self, sort_func):
        """Test sorting an array where all elements are the same."""
        data = [5] * 10
        result = get_sorted_result(sort_func, data)
        assert result == data
    
    @pytest.mark.parametrize("sort_func", ALL_SORTS)
    def test_large_array(self, sort_func):
        """Test sorting a larger array."""
        random.seed(123)
        data = [random.randint(0, 1000) for _ in range(100)]
        expected = sorted(data)
        result = get_sorted_result(sort_func, data)
        assert result == expected


class TestNegativeNumbers:
    """Test sorting algorithms with negative numbers."""
    
    @pytest.mark.parametrize("sort_func", NEGATIVE_SUPPORT)
    def test_negative_numbers(self, sort_func):
        """Test sorting with negative numbers."""
        data = [3, -1, 4, -1, 5, -9, 2, -6, 5, 3, -5]
        expected = sorted(data)
        result = get_sorted_result(sort_func, data)
        assert result == expected
    
    @pytest.mark.parametrize("sort_func", NEGATIVE_SUPPORT)
    def test_mixed_positive_negative(self, sort_func):
        """Test sorting with mixed positive and negative numbers."""
        data = [-5, 10, -3, 8, -1, 6, -7, 4, -9, 2]
        expected = sorted(data)
        result = get_sorted_result(sort_func, data)
        assert result == expected


class TestStability:
    """Test stability of sorting algorithms where applicable."""
    
    def test_stability_with_tuples(self):
        """Test stability with tuples (sort by first element, check second element order)."""
        # Create data with duplicate keys but unique values
        data = [(3, 'a'), (1, 'b'), (4, 'c'), (1, 'd'), (5, 'e'), (9, 'f'), (2, 'g'), (6, 'h')]
        
        for sort_func in STABLE_GENERAL_SORTS:
            result = get_sorted_result(sort_func, data)
            
            # Check that elements with same key maintain their relative order
            # For key=1: 'b' should come before 'd'
            key_1_elements = [x[1] for x in result if x[0] == 1]
            assert key_1_elements == ['b', 'd'], \
                f"{sort_func.__name__} is not stable: {key_1_elements}"
    
    def test_stability_with_objects(self):
        """Test stability with custom objects."""
        class Item:
            def __init__(self, key, value):
                self.key = key
                self.value = value
            
            def __repr__(self):
                return f"Item({self.key}, {self.value})"
            
            def __lt__(self, other):
                return self.key < other.key
            
            def __le__(self, other):
                return self.key <= other.key
            
            def __gt__(self, other):
                return self.key > other.key
            
            def __ge__(self, other):
                return self.key >= other.key
            
            def __eq__(self, other):
                return self.key == other.key and self.value == other.value
            
            def __ne__(self, other):
                return not self.__eq__(other)
        
        data = [Item(3, 'a'), Item(1, 'b'), Item(4, 'c'), Item(1, 'd'), Item(5, 'e')]
        
        for sort_func in STABLE_GENERAL_SORTS:
            result = get_sorted_result(sort_func, data)
            
            # Check stability for key=1
            key_1_values = [x.value for x in result if x.key == 1]
            assert key_1_values == ['b', 'd'], \
                f"{sort_func.__name__} is not stable: {key_1_values}"


class TestGeneratorFunctionality:
    """Test that sorting algorithms properly yield intermediate states."""
    
    @pytest.mark.parametrize("sort_func", ALL_SORTS)
    def test_generator_yields_states(self, sort_func):
        """Test that the generator yields SortState objects."""
        data = [3, 1, 4, 1, 5]
        states = list(sort_func(data))
        
        assert len(states) > 0
        assert all(isinstance(state, SortState) for state in states)
        
        # Check that the last state has the sorted array
        last_state = states[-1]
        assert last_state.array == sorted(data)
    
    @pytest.mark.parametrize("sort_func", ALL_SORTS)
    def test_generator_preserves_data(self, sort_func):
        """Test that the original data is not modified."""
        original = [3, 1, 4, 1, 5]
        data = original.copy()
        
        # Consume the generator
        list(sort_func(data))
        
        # Original data should not be modified
        assert data == original


class TestEdgeCases:
    """Test edge cases and special scenarios."""
    
    @pytest.mark.parametrize("sort_func", GENERAL_SORTS)
    def test_large_values(self, sort_func):
        """Test sorting with large values."""
        data = [10**9, 10**6, 10**3, 1, 10**12, 10**15]
        expected = sorted(data)
        result = get_sorted_result(sort_func, data)
        assert result == expected
    
    @pytest.mark.parametrize("sort_func", GENERAL_SORTS)
    def test_float_values(self, sort_func):
        """Test sorting with float values."""
        data = [3.14, 2.71, 1.41, 1.73, 2.24]
        expected = sorted(data)
        result = get_sorted_result(sort_func, data)
        assert result == expected
    
    @pytest.mark.parametrize("sort_func", GENERAL_SORTS)
    def test_string_values(self, sort_func):
        """Test sorting with string values."""
        data = ['banana', 'apple', 'cherry', 'date', 'elderberry']
        expected = sorted(data)
        result = get_sorted_result(sort_func, data)
        assert result == expected
    
    @pytest.mark.parametrize("sort_func", GENERAL_SORTS)
    def test_mixed_types(self, sort_func):
        """Test sorting with mixed comparable types."""
        # This should raise an error or work depending on implementation
        data = [1, 'a', 2, 'b']
        
        try:
            # This might raise TypeError for some algorithms
            result = get_sorted_result(sort_func, data)
            # If it doesn't raise an error, check if result is sorted
            # Note: This might not work for all type combinations
        except TypeError:
            # Expected for mixed types
            pass


class TestCountingRadixSpecific:
    """Test specific requirements for counting and radix sorts."""
    
    def test_counting_sort_non_negative(self):
        """Test that counting sort works with non-negative integers."""
        data = [3, 0, 4, 1, 5, 0, 2]
        expected = sorted(data)
        result = get_sorted_result(counting_sort, data)
        assert result == expected
    
    def test_radix_sort_non_negative(self):
        """Test that radix sort works with non-negative integers."""
        data = [170, 45, 75, 90, 802, 24, 2, 66]
        expected = sorted(data)
        result = get_sorted_result(radix_sort, data)
        assert result == expected
    
    def test_counting_sort_zero_elements(self):
        """Test counting sort with all zeros."""
        data = [0, 0, 0, 0, 0]
        result = get_sorted_result(counting_sort, data)
        assert result == data
    
    def test_radix_sort_single_digit(self):
        """Test radix sort with single digit numbers."""
        data = [9, 1, 5, 3, 7, 2, 8, 4, 6, 0]
        expected = sorted(data)
        result = get_sorted_result(radix_sort, data)
        assert result == expected


if __name__ == '__main__':
    pytest.main([__file__, '-v'])
