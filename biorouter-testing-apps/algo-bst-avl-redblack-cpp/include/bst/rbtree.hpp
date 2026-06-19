#pragma once
/// @file rbtree.hpp
/// Left-leaning red-black tree (template, header-only).
/// Implements insert / delete with O(log n) guarantees.

#include "common.hpp"
#include <stdexcept>
#include <iterator>
#include <cstddef>
#include <algorithm>

namespace bst {

template <typename K, typename V, typename Comp = DefaultComparator<K>>
class RBTree {
public:
    using NodeType = Node<K, V>;

private:
    NodeType* root_ = nullptr;
    std::size_t size_ = 0;
    Comp comp_;

    // ── colour helpers ────────────────────────────────────────────

    static Color color_of(const NodeType* n) {
        return n ? n->color : Color::BLACK;   // NIL leaves are black
    }
    static bool is_red(const NodeType* n) {
        return n && n->color == Color::RED;
    }

    // ── basic helpers ─────────────────────────────────────────────

    int cmp(const K& a, const K& b) const { return comp_(a, b); }

    static void update_height(NodeType* n) {
        if (n) n->height = 1 + std::max(n->left ? n->left->height : 0,
                                         n->right ? n->right->height : 0);
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

    // ── transplant (replace u with v) ─────────────────────────────

    void transplant(NodeType* u, NodeType* v) {
        if (!u->parent)             root_ = v;
        else if (u == u->parent->left)  u->parent->left  = v;
        else                            u->parent->right = v;
        if (v) v->parent = u->parent;
    }

    // ── insert fixup ──────────────────────────────────────────────

    void insert_fixup(NodeType* z) {
        while (is_red(z->parent)) {
            if (z->parent == z->parent->parent->left) {
                NodeType* uncle = z->parent->parent->right;
                if (is_red(uncle)) {                       // Case 1
                    z->parent->color   = Color::BLACK;
                    uncle->color       = Color::BLACK;
                    z->parent->parent->color = Color::RED;
                    z = z->parent->parent;
                } else {
                    if (z == z->parent->right) {            // Case 2
                        z = z->parent;
                        left_rotate(z);
                    }
                    z->parent->color          = Color::BLACK;  // Case 3
                    z->parent->parent->color  = Color::RED;
                    right_rotate(z->parent->parent);
                }
            } else {  // mirror: parent is right child of grandparent
                NodeType* uncle = z->parent->parent->left;
                if (is_red(uncle)) {
                    z->parent->color        = Color::BLACK;
                    uncle->color            = Color::BLACK;
                    z->parent->parent->color = Color::RED;
                    z = z->parent->parent;
                } else {
                    if (z == z->parent->left) {
                        z = z->parent;
                        right_rotate(z);
                    }
                    z->parent->color          = Color::BLACK;
                    z->parent->parent->color  = Color::RED;
                    left_rotate(z->parent->parent);
                }
            }
        }
        root_->color = Color::BLACK;
    }

    // ── delete fixup ──────────────────────────────────────────────
    //
    // x  is the node that "inherits" the extra black (may be nullptr).
    // x_parent is x's parent (needed because x can be NIL / nullptr).

    void delete_fixup(NodeType* x, NodeType* x_parent) {
        while (x != root_ && color_of(x) == Color::BLACK) {
            if (x == x_parent->left) {
                NodeType* w = x_parent->right;                // sibling
                if (color_of(w) == Color::RED) {             // Case 1
                    w->color           = Color::BLACK;
                    x_parent->color    = Color::RED;
                    left_rotate(x_parent);
                    w = x_parent->right;
                }
                if (color_of(w->left) == Color::BLACK &&
                    color_of(w->right) == Color::BLACK) {    // Case 2
                    w->color = Color::RED;
                    x = x_parent;
                    x_parent = x->parent;
                } else {
                    if (color_of(w->right) == Color::BLACK) {// Case 3
                        if (w->left) w->left->color = Color::BLACK;
                        w->color = Color::RED;
                        right_rotate(w);
                        w = x_parent->right;
                    }
                    w->color = x_parent->color;               // Case 4
                    x_parent->color = Color::BLACK;
                    if (w->right) w->right->color = Color::BLACK;
                    left_rotate(x_parent);
                    x = root_;
                }
            } else {  // mirror
                NodeType* w = x_parent->left;
                if (color_of(w) == Color::RED) {
                    w->color        = Color::BLACK;
                    x_parent->color = Color::RED;
                    right_rotate(x_parent);
                    w = x_parent->left;
                }
                if (color_of(w->right) == Color::BLACK &&
                    color_of(w->left) == Color::BLACK) {
                    w->color = Color::RED;
                    x = x_parent;
                    x_parent = x->parent;
                } else {
                    if (color_of(w->left) == Color::BLACK) {
                        if (w->right) w->right->color = Color::BLACK;
                        w->color = Color::RED;
                        left_rotate(w);
                        w = x_parent->left;
                    }
                    w->color = x_parent->color;
                    x_parent->color = Color::BLACK;
                    if (w->left) w->left->color = Color::BLACK;
                    right_rotate(x_parent);
                    x = root_;
                }
            }
        }
        if (x) x->color = Color::BLACK;
    }

    void destroy(NodeType* n) {
        if (!n) return;
        destroy(n->left);
        destroy(n->right);
        delete n;
    }

    // ── iterator ──────────────────────────────────────────────────
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

    RBTree() = default;
    ~RBTree() { clear(); }
    RBTree(const RBTree&)            = delete;
    RBTree& operator=(const RBTree&) = delete;

    void clear() { destroy(root_); root_ = nullptr; size_ = 0; }
    bool empty() const { return size_ == 0; }
    std::size_t size() const { return size_; }
    int height() const { return root_ ? root_->height : 0; }
    const NodeType* root() const { return root_; }

    void insert(const K& key, const V& value) {
        NodeType* z = new NodeType(key, value);
        z->color = Color::RED;

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
        if (!y)                          root_ = z;
        else if (cmp(key, y->key) < 0)   y->left  = z;
        else                             y->right = z;
        ++size_;
        insert_fixup(z);
        // update heights along path
        for (NodeType* n = z; n; n = n->parent) update_height(n);
    }

    bool erase(const K& key) {
        NodeType* z = find_node(key);
        if (!z) return false;

        NodeType* y = z;
        Color y_orig_color = y->color;
        NodeType* x = nullptr;
        NodeType* x_parent = nullptr;

        if (!z->left) {
            x = z->right;
            x_parent = z->parent;
            transplant(z, z->right);
        } else if (!z->right) {
            x = z->left;
            x_parent = z->parent;
            transplant(z, z->left);
        } else {
            y = minimum(z->right);
            y_orig_color = y->color;
            x = y->right;
            if (y->parent == z) {
                x_parent = y;
            } else {
                x_parent = y->parent;
                transplant(y, y->right);
                y->right = z->right;
                y->right->parent = y;
            }
            transplant(z, y);
            y->left = z->left;
            y->left->parent = y;
            y->color = z->color;
        }
        delete z;
        --size_;

        if (y_orig_color == Color::BLACK)
            delete_fixup(x, x_parent);

        // update heights
        if (root_) {
            // recompute all heights (simpler than tracking exact path)
            recompute_heights(root_);
        }
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

private:
    void recompute_heights(NodeType* n) {
        if (!n) return;
        recompute_heights(n->left);
        recompute_heights(n->right);
        update_height(n);
    }
};

} // namespace bst
