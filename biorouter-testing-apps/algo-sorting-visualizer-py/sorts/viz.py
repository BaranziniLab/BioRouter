"""
Terminal Visualizer for sorting algorithms.

Provides ANSI-based animation of sorting algorithms with colored bars.
"""

import time
import os
import sys
from typing import List, Any, Generator, Optional
from .base import SortState, ActionType


# ANSI color codes
class Colors:
    """ANSI color codes for terminal visualization."""
    RESET = "\033[0m"
    RED = "\033[31m"
    GREEN = "\033[32m"
    YELLOW = "\033[33m"
    BLUE = "\033[34m"
    MAGENTA = "\033[35m"
    CYAN = "\033[36m"
    WHITE = "\033[37m"
    BOLD = "\033[1m"
    UNDERLINE = "\033[4m"
    
    # Background colors
    BG_RED = "\033[41m"
    BG_GREEN = "\033[42m"
    BG_YELLOW = "\033[43m"
    BG_BLUE = "\033[44m"
    BG_MAGENTA = "\033[45m"
    BG_CYAN = "\033[46m"
    BG_WHITE = "\033[47m"


def clear_screen():
    """Clear the terminal screen."""
    os.system('cls' if os.name == 'nt' else 'clear')


def hide_cursor():
    """Hide the terminal cursor."""
    sys.stdout.write("\033[?25l")
    sys.stdout.flush()


def show_cursor():
    """Show the terminal cursor."""
    sys.stdout.write("\033[?25h")
    sys.stdout.flush()


def move_cursor_to_top():
    """Move cursor to the top of the terminal."""
    sys.stdout.write("\033[H")
    sys.stdout.flush()


def get_terminal_size():
    """Get terminal dimensions."""
    try:
        columns, rows = os.get_terminal_size()
        return columns, rows
    except OSError:
        return 80, 24


def create_bar(value: int, max_value: int, width: int = 50) -> str:
    """
    Create a visual bar representation of a value.
    
    Args:
        value: The value to represent
        max_value: Maximum value for scaling
        width: Width of the bar in characters
        
    Returns:
        String representation of the bar
    """
    if max_value == 0:
        return "│" + " " * width + "│"
    
    bar_length = int((value / max_value) * width)
    bar = "█" * bar_length + "░" * (width - bar_length)
    return f"│{bar}│"


def create_colored_bar(value: int, max_value: int, width: int = 50, 
                      color: str = Colors.GREEN, highlight: bool = False) -> str:
    """
    Create a colored visual bar representation of a value.
    
    Args:
        value: The value to represent
        max_value: Maximum value for scaling
        width: Width of the bar in characters
        color: ANSI color code
        highlight: Whether to highlight this bar
        
    Returns:
        Colored string representation of the bar
    """
    if max_value == 0:
        return "│" + " " * width + "│"
    
    bar_length = int((value / max_value) * width)
    
    if highlight:
        bar = f"{Colors.BG_YELLOW}{'█' * bar_length}{Colors.RESET}{'░' * (width - bar_length)}"
    else:
        bar = f"{color}{'█' * bar_length}{Colors.RESET}{'░' * (width - bar_length)}"
    
    return f"│{bar}│"


def visualize_sorting(sort_func, data: List[Any], speed: float = 0.1, 
                     show_stats: bool = True) -> List[Any]:
    """
    Visualize a sorting algorithm in the terminal.
    
    Args:
        sort_func: Sorting function that yields SortState objects
        data: List of elements to sort
        speed: Delay between frames in seconds
        show_stats: Whether to show statistics
        
    Returns:
        Sorted list
    """
    if not data:
        return []
    
    max_value = max(data)
    terminal_width, terminal_height = get_terminal_size()
    
    # Calculate bar width based on terminal width
    # Leave room for borders, value display, and padding
    bar_width = min(50, terminal_width - 20)
    
    # Prepare data
    arr = data.copy()
    
    # Clear screen and hide cursor
    clear_screen()
    hide_cursor()
    
    try:
        last_state = None
        frame_count = 0
        
        for state in sort_func(arr):
            last_state = state
            frame_count += 1
            
            # Move cursor to top
            move_cursor_to_top()
            
            # Print header
            print(f"{Colors.BOLD}{Colors.CYAN}Sorting Algorithm Visualizer{Colors.RESET}")
            print(f"{Colors.YELLOW}Algorithm: {state.algorithm}{Colors.RESET}")
            print(f"{Colors.WHITE}Frame: {frame_count}{Colors.RESET}")
            print()
            
            # Print array visualization
            for i, value in enumerate(state.array):
                # Determine if this index is being compared or swapped
                is_highlighted = False
                is_compared = False
                is_swapped = False
                
                if state.action.indices and i in state.action.indices:
                    is_highlighted = True
                    if state.action.action_type == ActionType.COMPARE:
                        is_compared = True
                    elif state.action.action_type == ActionType.SWAP:
                        is_swapped = True
                
                # Choose color based on action
                if is_swapped:
                    color = Colors.RED
                elif is_compared:
                    color = Colors.YELLOW
                else:
                    color = Colors.GREEN
                
                # Create and print bar
                bar = create_colored_bar(value, max_value, bar_width, color, is_highlighted)
                print(f"{i:3d} {bar} {value:3d}")
            
            # Print action description
            print()
            if state.action.action_type == ActionType.COMPARE:
                print(f"{Colors.YELLOW}Comparing indices: {state.action.indices}{Colors.RESET}")
            elif state.action.action_type == ActionType.SWAP:
                print(f"{Colors.RED}Swapping indices: {state.action.indices}{Colors.RESET}")
            elif state.action.action_type == ActionType.OVERWRITE:
                print(f"{Colors.BLUE}Overwriting index: {state.action.indices}{Colors.RESET}")
            elif state.action.action_type == ActionType.ACCESS:
                print(f"{Colors.WHITE}Accessing index: {state.action.indices}{Colors.RESET}")
            
            # Print stats if requested
            if show_stats:
                print()
                print(f"{Colors.WHITE}Press Ctrl+C to stop{Colors.RESET}")
            
            # Delay for animation
            time.sleep(speed)
        
        # Print final state
        if last_state:
            move_cursor_to_top()
            print(f"{Colors.BOLD}{Colors.GREEN}Sorting Complete!{Colors.RESET}")
            print(f"{Colors.YELLOW}Algorithm: {last_state.algorithm}{Colors.RESET}")
            print(f"{Colors.WHITE}Frames: {frame_count}{Colors.RESET}")
            print()
            
            # Print final array
            for i, value in enumerate(last_state.array):
                bar = create_colored_bar(value, max_value, bar_width, Colors.GREEN)
                print(f"{i:3d} {bar} {value:3d}")
            
            print()
            print(f"{Colors.GREEN}Array is now sorted!{Colors.RESET}")
        
        return last_state.array if last_state else arr
        
    except KeyboardInterrupt:
        print(f"\n{Colors.RED}Visualization interrupted by user{Colors.RESET}")
        return arr
    finally:
        show_cursor()


def print_array_snapshot(array: List[Any], action=None, algorithm: str = "", 
                        max_width: int = 60) -> str:
    """
    Create a string representation of an array snapshot.
    
    Args:
        array: The array to display
        action: The current action
        algorithm: Name of the algorithm
        max_width: Maximum width for bars
        
    Returns:
        String representation
    """
    if not array:
        return "[]"
    
    max_value = max(array)
    result = []
    
    if algorithm:
        result.append(f"Algorithm: {algorithm}")
    
    for i, value in enumerate(array):
        # Create a simple text bar
        if max_value > 0:
            bar_length = int((value / max_value) * 20)
            bar = "█" * bar_length + "░" * (20 - bar_length)
        else:
            bar = "░" * 20
        
        # Highlight if in action
        highlight = ""
        if action and action.indices and i in action.indices:
            highlight = " <--"
        
        result.append(f"{i:3d}: {bar} {value:3d}{highlight}")
    
    return "\n".join(result)
