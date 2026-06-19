/// @file test_avl.cpp  — Unit tests for the AVL tree.
#include "test_framework.hpp"
#include "bst/avl.hpp"
#include "bst/verify.hpp"

using Comp = bst::DefaultComparator<int>;

static bst::VerifyResult verify(const bst::AVL<int,int>& t) {
    auto p = bst::verify_parents(t.root(),
                 static_cast<const bst::Node<int,int>*>(nullptr));
    if (!p.ok) return p;
    return bst::verify_avl<int,int>(t.root(), Comp{});
}

// ── basic insert / find ────────────────────────────────────────────

TEST(avl_insert_find, "AVL: insert and find") {
    bst::AVL<int,int> t;
    t.insert(10,100); t.insert(20,200); t.insert(5,50);
    ASSERT(t.find(10) && *t.find(10) == 100);
    ASSERT_EQ(t.find(99), nullptr);
    ASSERT(t.size() == 3);
    ASSERT(verify(t).ok);
}

// ── LL rotation ────────────────────────────────────────────────────

TEST(avl_ll, "AVL: LL rotation") {
    bst::AVL<int,int> t;
    t.insert(3,0); t.insert(2,0); t.insert(1,0);
    // After rotation, root should be 2
    ASSERT(t.root()->key == 2);
    ASSERT_EQ(t.height(), 2);
    ASSERT(verify(t).ok);
}

// ── RR rotation ────────────────────────────────────────────────────

TEST(avl_rr, "AVL: RR rotation") {
    bst::AVL<int,int> t;
    t.insert(1,0); t.insert(2,0); t.insert(3,0);
    ASSERT(t.root()->key == 2);
    ASSERT_EQ(t.height(), 2);
    ASSERT(verify(t).ok);
}

// ── LR rotation ────────────────────────────────────────────────────

TEST(avl_lr, "AVL: LR rotation") {
    bst::AVL<int,int> t;
    t.insert(3,0); t.insert(1,0); t.insert(2,0);
    ASSERT(t.root()->key == 2);
    ASSERT_EQ(t.height(), 2);
    ASSERT(verify(t).ok);
}

// ── RL rotation ────────────────────────────────────────────────────

TEST(avl_rl, "AVL: RL rotation") {
    bst::AVL<int,int> t;
    t.insert(1,0); t.insert(3,0); t.insert(2,0);
    ASSERT(t.root()->key == 2);
    ASSERT_EQ(t.height(), 2);
    ASSERT(verify(t).ok);
}

// ── sorted insertion stays O(log n) ────────────────────────────────

TEST(avl_sorted_height, "AVL: sorted insertion height ≤ 1.44 log2(n)") {
    bst::AVL<int,int> t;
    const int N = 1000;
    for (int i = 0; i < N; ++i) t.insert(i, 0);
    double max_h = 1.44 * std::log2(N + 2);
    ASSERT_LE(t.height(), (int)max_h + 1);
    ASSERT_EQ(t.size(), (std::size_t)N);
    ASSERT(verify(t).ok);
}

// ── delete ─────────────────────────────────────────────────────────

TEST(avl_del_leaf, "AVL: delete leaf") {
    bst::AVL<int,int> t;
    t.insert(10,0); t.insert(5,0); t.insert(15,0);
    ASSERT(t.erase(5));
    ASSERT_EQ(t.find(5), nullptr);
    ASSERT(t.size() == 2);
    ASSERT(verify(t).ok);
}

TEST(avl_del_two_children, "AVL: delete node with two children") {
    bst::AVL<int,int> t;
    t.insert(10,0); t.insert(5,0); t.insert(15,0); t.insert(3,0); t.insert(7,0);
    ASSERT(t.erase(5));
    ASSERT_EQ(t.find(5), nullptr);
    ASSERT(t.size() == 4);
    ASSERT(verify(t).ok);
}

TEST(avl_del_root, "AVL: delete root") {
    bst::AVL<int,int> t;
    t.insert(10,0); t.insert(5,0); t.insert(15,0);
    ASSERT(t.erase(10));
    ASSERT_EQ(t.find(10), nullptr);
    ASSERT(verify(t).ok);
}

TEST(avl_del_rebalance, "AVL: delete triggers rebalance") {
    bst::AVL<int,int> t;
    // Build a tree where deleting one node forces a rebalance
    t.insert(10,0); t.insert(5,0); t.insert(15,0); t.insert(3,0); t.insert(7,0);
    t.insert(12,0); t.insert(18,0); t.insert(1,0);
    ASSERT(verify(t).ok);
    t.erase(18);
    ASSERT(verify(t).ok);
    t.erase(15);
    ASSERT(verify(t).ok);
    t.erase(12);
    ASSERT(verify(t).ok);
}

// ── in-order ───────────────────────────────────────────────────────

TEST(avl_inorder, "AVL: in-order traversal yields sorted keys") {
    bst::AVL<int,int> t;
    int keys[] = {50, 30, 70, 20, 40, 60, 80, 10, 25, 35, 45};
    for (int k : keys) t.insert(k, 0);
    int prev = -1;
    for (auto& n : t) {
        ASSERT_GT(n.key, prev);
        prev = n.key;
    }
    ASSERT(verify(t).ok);
}

// ── successor / predecessor ────────────────────────────────────────

TEST(avl_succ_pred, "AVL: successor and predecessor") {
    bst::AVL<int,int> t;
    for (int i = 0; i < 10; ++i) t.insert(i*2, 0);
    auto s = t.successor(4);
    ASSERT(s && *s == 6);
    auto p = t.predecessor(4);
    ASSERT(p && *p == 2);
    ASSERT_EQ(t.successor(18), nullptr);
    ASSERT_EQ(t.predecessor(0), nullptr);
}

// ── duplicate key ──────────────────────────────────────────────────

TEST(avl_dup, "AVL: duplicate key updates value") {
    bst::AVL<int,int> t;
    t.insert(5, 10); t.insert(5, 20);
    ASSERT(t.size() == 1);
    ASSERT(t.find(5) && *t.find(5) == 20);
    ASSERT(verify(t).ok);
}
