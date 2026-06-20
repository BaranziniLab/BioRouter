#pragma once
/// @file avl.hpp
/// Self-balancing AVL tree (template, header-only).

#include "common.hpp"
#include <stdexcept>
#include <iterator>
#include <cstddef>
#include <algorithm>

namespace bst {

template <typename K, typename V, typename Comp = DefaultComparator<K>>
class AVL {
public:
    using NodeType = Node<K, V>;

private:
    NodeType* root_ = nullptr;
    std::size_t size_ = 0;
    Comp comp_;

    // ── helpers ───────────────────────────────────────────────────

    int cmp(const K& a, const K& b) const { return comp_(a, b); }

    static int ht(const NodeType* n) { return n ? n->height : 0; }

    static void update_height(NodeType* n) {
        if (n) n->height = 1 + std::max(ht(n->left), ht(n->right));
    }

    /// Balance factor = left height − right height.
    static int bf(const NodeType* n) {
        return n ? ht(n->left) - ht(n->right) : 0;
    }

    NodeType* find_node(const K& key) const {
        NodeType* n = root_;
        while (n) {
            int c = cmp(key, n->key);
            if (c < 0)      n = n->left;
            else if (c > 0) n = n->right;
            else             return n;
        }
        return nullptr;
    }

    static NodeType* minimum(NodeType* n) {
        while (n && n->left) n = n->left;
        return n;
    }
    static NodeType* maximum(NodeType* n) {
        while (n && n->right) n = n->right;
        return n;
    }

    // ── rotations ─────────────────────────────────────────────────
    //
    //       y            x
    //      / \          / \         (right rotation on y)
    //     x   C  →     A   y
    //    / \              / \
    //   A   B            B   C
    //
    void right_rotate(NodeType* y) {
        NodeType* x = y->left;
        y->left = x->right;
        if (x->right) x->right->parent = y;
        x->parent = y->parent;
        if (!y->parent)          root_ = x;
        else if (y == y->parent->left)  y->parent->left  = x;
        else                            y->parent->right = x;
        x->right = y;
        y->parent = x;
        update_height(y);
        update_height(x);
    }

    void left_rotate(NodeType* x) {
        NodeType* y = x->right;
        x->right = y->left;
        if (y->left) y->left->parent = x;
        y->parent = x->parent;
        if (!x->parent)          root_ = y;
        else if (x == x->parent->left)  x->parent->left  = y;
        else                            x->parent->right = y;
        y->left = x;
        x->parent = y;
        update_height(x);
        update_height(y);
    }

    /// Rebalance the subtree rooted at `n` (single step).
    void rebalance(NodeType* n) {
        if (!n) return;
        update_height(n);
        int b = bf(n);
        if (b > 1) {        // left-heavy
            if (bf(n->left) < 0)   // LR case
                left_rotate(n->left);
            right_rotate(n);       // LL case (or after LR fix)
        } else if (b < -1) { // right-heavy
            if (bf(n->right) > 0)  // RL case
                right_rotate(n->right);
            left_rotate(n);        // RR case (or after RL fix)
        }
    }

    /// Walk from `n` up to the root, rebalancing each ancestor.
    void fix_up(NodeType* n) {
        while (n) { rebalance(n); n = n->parent; }
    }

    void transplant(NodeType* u, NodeType* v) {
        if (!u->parent)             root_ = v;
        else if (u == u->parent->left)  u->parent->left  = v;
        else                            u->parent->right = v;
        if (v) v->parent = u->parent;
    }

    void destroy(NodeType* n) {
        if (!n) return;
        destroy(n->left);
        destroy(n->right);
        delete n;
    }

    // ── iterator (identical to BST) ───────────────────────────────
public:
    class iterator {
    public:
        using iterator_category = std::bidirectional_iterator_tag;
        using value_type        = NodeType;
        using difference_type   = std::ptrdiff_t;
        using pointer           = NodeType*;
        using reference         = NodeType&;
    private:
        pointer node_ = nullptr;
        void advance() {
            if (node_->right) {
                node_ = node_->right;
                while (node_->left) node_ = node_->left;
            } else {
                pointer c = node_;
                node_ = node_->parent;
                while (node_ && node_->right == c) { c = node_; node_ = node_->parent; }
            }
        }
        void retreat() {
            if (node_->left) {
                node_ = node_->left;
                while (node_->right) node_ = node_->right;
            } else {
                pointer c = node_;
                node_ = node_->parent;
                while (node_ && node_->left == c) { c = node_; node_ = node_->parent; }
            }
        }
    public:
        iterator() = default;
        explicit iterator(pointer p) : node_(p) {}
        reference operator*()  const { return *node_; }
        pointer   operator->() const { return  node_; }
        iterator& operator++() { advance();  return *this; }
        iterator  operator++(int) { auto t = *this; advance(); return t; }
        iterator& operator--() { retreat();  return *this; }
        iterator  operator--(int) { auto t = *this; retreat(); return t; }
        bool operator==(const iterator& o) const { return node_ == o.node_; }
        bool operator!=(const iterator& o) const { return node_ != o.node_; }
    };

    // ── public API ────────────────────────────────────────────────

    AVL() = default;
    ~AVL() { clear(); }
    AVL(const AVL&)            = delete;
    AVL& operator=(const AVL&) = delete;

    void clear() { destroy(root_); root_ = nullptr; size_ = 0; }
    bool empty() const { return size_ == 0; }
    std::size_t size() const { return size_; }
    int height() const { return ht(root_); }
    const NodeType* root() const { return root_; }

    void insert(const K& key, const V& value) {
        NodeType* z = new NodeType(key, value);
        NodeType* y = nullptr;
        NodeType* x = root_;
        while (x) {
            y = x;
            int c = cmp(key, x->key);
            if (c < 0)      x = x->left;
            else if (c > 0) x = x->right;
            else { x->value = value; delete z; return; }
        }
        z->parent = y;
        if (!y)                root_ = z;
        else if (cmp(key, y->key) < 0) y->left  = z;
        else                           y->right = z;
        ++size_;
        fix_up(z);
    }

    bool erase(const K& key) {
        NodeType* z = find_node(key);
        if (!z) return false;

        NodeType* fix_from = nullptr;

        if (!z->left) {
            fix_from = z->parent;
            transplant(z, z->right);
        } else if (!z->right) {
            fix_from = z->parent;
            transplant(z, z->left);
        } else {
            NodeType* y = minimum(z->right);
            if (y->parent != z) {
                fix_from = y->parent;
                transplant(y, y->right);
                y->right = z->right;
                y->right->parent = y;
            } else {
                fix_from = y;
            }
            transplant(z, y);
            y->left = z->left;
            y->left->parent = y;
            update_height(y);
        }
        delete z;
        --size_;
        fix_up(fix_from);
        return true;
    }

    V* find(const K& key) const {
        NodeType* n = find_node(key);
        return n ? &n->value : nullptr;
    }

    const K& min_key() const {
        if (!root_) throw std::runtime_error("min_key on empty tree");
        return minimum(root_)->key;
    }
    const K& max_key() const {
        if (!root_) throw std::runtime_error("max_key on empty tree");
        return maximum(root_)->key;
    }

    const K* successor(const K& key) const {
        NodeType* n = find_node(key);
        if (!n) return nullptr;
        if (n->right) return &minimum(n->right)->key;
        NodeType* p = n->parent;
        while (p && n == p->right) { n = p; p = p->parent; }
        return p ? &p->key : nullptr;
    }

    const K* predecessor(const K& key) const {
        NodeType* n = find_node(key);
        if (!n) return nullptr;
        if (n->left) return &maximum(n->left)->key;
        NodeType* p = n->parent;
        while (p && n == p->left) { n = p; p = p->parent; }
        return p ? &p->key : nullptr;
    }

    iterator begin() const { return iterator(minimum(root_)); }
    iterator end()   const { return iterator(nullptr); }
};

} // namespace bst
