#pragma once
/// @file bst.hpp
/// Unbalanced binary search tree (template, header-only).

#include "common.hpp"
#include <stdexcept>
#include <iterator>
#include <cstddef>
#include <algorithm>

namespace bst {

template <typename K, typename V, typename Comp = DefaultComparator<K>>
class BST {
public:
    using NodeType = Node<K, V>;

private:
    NodeType* root_ = nullptr;
    std::size_t size_ = 0;
    Comp comp_;

    // ── helpers ───────────────────────────────────────────────────

    int cmp(const K& a, const K& b) const { return comp_(a, b); }

    static int node_height(const NodeType* n) {
        return n ? n->height : 0;
    }
    static void update_height(NodeType* n) {
        if (n)
            n->height = 1 + std::max(node_height(n->left), node_height(n->right));
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

    /// Walk up from `n` updating heights.
    void update_ancestors(NodeType* n) {
        while (n) { update_height(n); n = n->parent; }
    }

    static NodeType* minimum(NodeType* n) {
        while (n && n->left) n = n->left;
        return n;
    }
    static NodeType* maximum(NodeType* n) {
        while (n && n->right) n = n->right;
        return n;
    }

    /// Replace `u` with `v` in the tree (parent pointer wiring only).
    void transplant(NodeType* u, NodeType* v) {
        if (!u->parent)            root_ = v;
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

    // ── in-order iterator ─────────────────────────────────────────
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
        void advance() {  // successor
            if (node_->right) {
                node_ = node_->right;
                while (node_->left) node_ = node_->left;
            } else {
                pointer child = node_;
                node_ = node_->parent;
                while (node_ && node_->right == child) {
                    child = node_;
                    node_ = node_->parent;
                }
            }
        }
        void retreat() {  // predecessor
            if (node_->left) {
                node_ = node_->left;
                while (node_->right) node_ = node_->right;
            } else {
                pointer child = node_;
                node_ = node_->parent;
                while (node_ && node_->left == child) {
                    child = node_;
                    node_ = node_->parent;
                }
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

    BST() = default;
    ~BST() { clear(); }
    BST(const BST&)            = delete;
    BST& operator=(const BST&) = delete;

    void clear() { destroy(root_); root_ = nullptr; size_ = 0; }
    bool empty() const { return size_ == 0; }
    std::size_t size() const { return size_; }
    int height() const { return node_height(root_); }

    /// Read-only access to the root (for the verify harness).
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
            else {  // duplicate key → update value
                x->value = value;
                delete z;
                return;
            }
        }
        z->parent = y;
        if (!y)                root_ = z;
        else if (cmp(key, y->key) < 0) y->left  = z;
        else                           y->right = z;
        ++size_;
        update_ancestors(z);
    }

    bool erase(const K& key) {
        NodeType* z = find_node(key);
        if (!z) return false;

        if (!z->left) {
            transplant(z, z->right);
        } else if (!z->right) {
            transplant(z, z->left);
        } else {
            NodeType* y = minimum(z->right);
            if (y->parent != z) {
                transplant(y, y->right);
                y->right = z->right;
                y->right->parent = y;
            }
            transplant(z, y);
            y->left = z->left;
            y->left->parent = y;
            update_height(y);
        }
        NodeType* parent = z->parent;
        delete z;
        --size_;
        update_ancestors(parent);
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

    /// Returns a pointer to the successor key, or nullptr.
    const K* successor(const K& key) const {
        NodeType* n = find_node(key);
        if (!n) return nullptr;
        if (n->right) return &minimum(n->right)->key;
        NodeType* p = n->parent;
        while (p && n == p->right) { n = p; p = p->parent; }
        return p ? &p->key : nullptr;
    }

    /// Returns a pointer to the predecessor key, or nullptr.
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
