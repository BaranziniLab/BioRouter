#pragma once
/// @file common.hpp
/// Shared node type and comparator for all BST implementations.

#include <cstdint>
#include <functional>
#include <utility>

namespace bst {

/// Color tag for red-black tree nodes.
enum class Color : uint8_t { RED = 0, BLACK = 1 };

/// A single node in a BST / AVL / Red-Black tree.
/// All three implementations share this layout so the verify harness can
/// operate generically. Fields not needed by a particular tree variant
/// (e.g. `height` for BST, `color` for AVL) are left at their default
/// values and ignored.
template <typename K, typename V>
struct Node {
    K     key;
    V     value;
    Node* left   = nullptr;
    Node* right  = nullptr;
    Node* parent = nullptr;
    int   height = 1;                    ///< AVL subtree height (1 = leaf).
    Color color  = Color::RED;           ///< RB color (new nodes are red).

    Node() = default;
    Node(const K& k, const V& v) : key(k), value(v) {}
    Node(K&& k, V&& v) : key(std::move(k)), value(std::move(v)) {}
};

/// Three-way comparator: returns <0, 0, or >0.
template <typename K>
struct DefaultComparator {
    int operator()(const K& a, const K& b) const {
        if (a < b) return -1;
        if (b < a) return  1;
        return 0;
    }
};

} // namespace bst
