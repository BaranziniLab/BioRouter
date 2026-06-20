#pragma once
/// @file verify.hpp
/// Invariant-checking harness for BST, AVL, and Red-Black trees.
///
/// Each function walks the tree recursively and returns a VerifyResult
/// containing pass/fail status and an optional diagnostic message.
///
/// Usage (in tests):
///   auto r = bst::verify_bst_order(tree.root(), bst::DefaultComparator<int>{});
///   assert(r.ok);

#include "common.hpp"
#include <string>
#include <algorithm>

namespace bst {

// ── result type ────────────────────────────────────────────────────

struct VerifyResult {
    bool ok = true;
    std::string msg;

    static VerifyResult pass() { return {}; }
    static VerifyResult fail(std::string m) {
        VerifyResult r; r.ok = false; r.msg = std::move(m); return r;
    }
    explicit operator bool() const { return ok; }
};

// ── BST ordering ──────────────────────────────────────────────────
/// Checks that every node's key satisfies: lo < key < hi
/// where lo/hi are inherited bounds from ancestors.
template <typename K, typename V, typename Comp>
VerifyResult verify_bst_order(const Node<K,V>* n, const Comp& comp,
                              const K* lo = nullptr, const K* hi = nullptr)
{
    if (!n) return VerifyResult::pass();
    if (lo && comp(n->key, *lo) <= 0)
        return VerifyResult::fail("BST order: key violates lower bound");
    if (hi && comp(n->key, *hi) >= 0)
        return VerifyResult::fail("BST order: key violates upper bound");
    auto lr = verify_bst_order(n->left, comp, lo, &n->key);
    if (!lr.ok) return lr;
    return verify_bst_order(n->right, comp, &n->key, hi);
}

// ── parent-pointer consistency ─────────────────────────────────────
template <typename K, typename V>
VerifyResult verify_parents(const Node<K,V>* n, const Node<K,V>* expected)
{
    if (!n) return VerifyResult::pass();
    if (n->parent != expected)
        return VerifyResult::fail("Parent pointer mismatch");
    auto lr = verify_parents(n->left, n);
    if (!lr.ok) return lr;
    return verify_parents(n->right, n);
}

// ── AVL invariants ────────────────────────────────────────────────
/// Checks BST ordering + correct heights + |balance factor| ≤ 1.
template <typename K, typename V, typename Comp>
VerifyResult verify_avl(const Node<K,V>* n, const Comp& comp)
{
    if (!n) return VerifyResult::pass();
    // BST order (entire subtree at once)
    auto bo = verify_bst_order(n, comp);
    if (!bo.ok) return bo;
    return verify_avl_heights(n);
}

/// Height / balance-factor check (internal, called on each node).
template <typename K, typename V>
VerifyResult verify_avl_heights(const Node<K,V>* n)
{
    if (!n) return VerifyResult::pass();
    int lh = n->left  ? n->left->height  : 0;
    int rh = n->right ? n->right->height : 0;
    int expected = 1 + std::max(lh, rh);
    if (n->height != expected)
        return VerifyResult::fail("AVL height mismatch at key " +
            std::to_string(n->key) + ": got " + std::to_string(n->height) +
            ", expected " + std::to_string(expected));
    int bf = lh - rh;
    if (bf < -1 || bf > 1)
        return VerifyResult::fail("AVL balance factor " + std::to_string(bf) +
            " at key " + std::to_string(n->key));
    auto lr = verify_avl_heights(n->left);
    if (!lr.ok) return lr;
    return verify_avl_heights(n->right);
}

// ── Red-Black tree properties ─────────────────────────────────────
///
/// Properties checked:
///  P1 — every node is RED or BLACK  (always true by construction)
///  P2 — root is BLACK
///  P3 — NIL leaves are BLACK        (modelled as nullptr)
///  P4 — RED node ⇒ both children BLACK
///  P5 — equal black-height on every root-to-NIL path
///  + BST ordering

template <typename K, typename V>
struct RBCheck {
    int bh;            // black-height of this subtree
    VerifyResult result;
};

template <typename K, typename V, typename Comp>
RBCheck<K,V> verify_rb_impl(const Node<K,V>* n, const Comp& comp,
                             const K* lo, const K* hi)
{
    if (!n) return {1, VerifyResult::pass()};   // NIL leaf

    // BST order
    if (lo && comp(n->key, *lo) <= 0)
        return {0, VerifyResult::fail("RB BST order violated (lower)")};
    if (hi && comp(n->key, *hi) >= 0)
        return {0, VerifyResult::fail("RB BST order violated (upper)")};

    // P4: red node → children black
    if (n->color == Color::RED) {
        if (n->left  && n->left->color  == Color::RED)
            return {0, VerifyResult::fail("RB red-red left at key " +
                std::to_string(n->key))};
        if (n->right && n->right->color == Color::RED)
            return {0, VerifyResult::fail("RB red-red right at key " +
                std::to_string(n->key))};
    }

    auto lr = verify_rb_impl(n->left,  comp, lo, &n->key);
    if (!lr.result.ok) return {0, lr.result};
    auto rr = verify_rb_impl(n->right, comp, &n->key, hi);
    if (!rr.result.ok) return {0, rr.result};

    // P5: equal black-height
    if (lr.bh != rr.bh)
        return {0, VerifyResult::fail("RB black-height mismatch at key " +
            std::to_string(n->key))};

    int bh = lr.bh + (n->color == Color::BLACK ? 1 : 0);
    return {bh, VerifyResult::pass()};
}

template <typename K, typename V, typename Comp>
VerifyResult verify_rbtree(const Node<K,V>* root, const Comp& comp)
{
    if (root && root->color != Color::BLACK)
        return VerifyResult::fail("RB: root is not black");
    const K* lo = nullptr;
    const K* hi = nullptr;
    return verify_rb_impl(root, comp, lo, hi).result;
}

// ── size check (walks entire tree and counts) ─────────────────────
template <typename K, typename V>
std::size_t count_nodes(const Node<K,V>* n) {
    if (!n) return 0;
    return 1 + count_nodes(n->left) + count_nodes(n->right);
}

} // namespace bst
