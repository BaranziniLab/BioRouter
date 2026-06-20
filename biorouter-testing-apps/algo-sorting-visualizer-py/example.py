#!/usr/bin/env python3
"""
Example script demonstrating the sorting algorithm visualizer.

This script shows how to use the sorting algorithms programmatically.
"""

from sorts import bubble_sort, quick_sort, merge_sort
from sorts.viz import visualize_sorting, print_array_snapshot
from sorts.instrument import get_algorithm_info


def demo_sorting():
    """Demonstrate sorting algorithms."""
    print("Sorting Algorithm Visualizer Demo")
    print("=" * 40)
    
    # Sample data
    data = [64, 34, 25, 12, 22, 11, 90]
    print(f"\nOriginal array: {data}")
    
    # Sort with bubble sort
    print("\n1. Bubble Sort:")
    sorted_data = []
    for state in bubble_sort(data):
        sorted_data = state.array
    print(f"Sorted array: {sorted_data}")
    
    # Sort with quick sort
    print("\n2. Quick Sort (median-of-three):")
    sorted_data = []
    for state in quick_sort(data):
        sorted_data = state.array
    print(f"Sorted array: {sorted_data}")
    
    # Sort with merge sort
    print("\n3. Merge Sort:")
    sorted_data = []
    for state in merge_sort(data):
        sorted_data = state.array
    print(f"Sorted array: {sorted_data}")


def demo_algorithm_info():
    """Show algorithm information."""
    print("\nAlgorithm Information")
    print("=" * 40)
    
    algorithms = ['bubble', 'quick', 'merge', 'heap', 'counting']
    
    for algo in algorithms:
        info = get_algorithm_info(algo)
        print(f"\n{algo.upper()}:")
        print(f"  Time Complexity: {info['time_complexity']}")
        print(f"  Space Complexity: {info['space_complexity']}")
        print(f"  Stable: {'Yes' if info['stable'] else 'No'}")


def demo_visualization():
    """Demonstrate terminal visualization."""
    print("\nTerminal Visualization Demo")
    print("=" * 40)
    print("This will open a terminal visualization.")
    print("Press Ctrl+C to stop the visualization.\n")
    
    data = [64, 34, 25, 12, 22, 11, 90, 55, 33, 11]
    
    try:
        # Visualize bubble sort with slow speed
        print("Visualizing bubble sort...")
        visualize_sorting(bubble_sort, data, speed=0.3, show_stats=True)
    except KeyboardInterrupt:
        print("\nVisualization stopped.")


if __name__ == '__main__':
    demo_sorting()
    demo_algorithm_info()
    
    # Uncomment to see terminal visualization
    # demo_visualization()
