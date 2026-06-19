/// @file test_rbtree.cpp  — Unit tests for the red-black tree.
#include "test_framework.hpp"
#include "bst/rbtree.hpp"
#include "bst/verify.hpp"

using Comp = bst::DefaultComparator<int>;

static bst::VerifyResult verify(const bst::RBTree<int,int>& t) {
    auto p = bst::verify_parents(t.root(),
                 static_cast<const bst::Node<int,int>*>(nullptr));
    if (!p.ok) return p;
    return bst::verify_rbtree<int,int>(t.root(), Comp{});
}

// ── basic insert / find ────────────────────────────────────────────

TEST(rb_insert_find, "RB: insert and find") {
    bst::RBTree<int,int> t;
    t.insert(10,100); t.insert(20,200); t.insert(5,50);
    ASSERT(t.find(10) && *t.find(10) == 100);
    ASSERT_EQ(t.find(99), nullptr);
    ASSERT(t.size() == 3);
    ASSERT(verify(t).ok);
}

// ── root is black ──────────────────────────────────────────────────

TEST(rb_root_black, "RB: root is always black") {
    bst::RBTree<int,int> t;
    for (int i = 1; i <= 20; ++i) {
        t.insert(i, 0);
        ASSERT(t.root());
        ASSERT_EQ((int)t.root()->color, (int)bst::Color::BLACK);
    }
    ASSERT(verify(t).ok);
}

// ── no red-red violations after inserts ────────────────────────────

TEST(rb_no_red_red, "RB: no red-red after sequential inserts") {
    bst::RBTree<int,int> t;
    for (int i = 0; i < 100; ++i) {
        t.insert(i, 0);
        auto r = verify(t);
        ASSERT_MSG(r.ok, r.msg);
    }
    ASSERT(verify(t).ok);
}

// ── sequential insertion height check ──────────────────────────────

TEST(rb_height, "RB: height ≤ 2 log2(n+1)") {
    bst::RBTree<int,int> t;
    const int N = 1000;
    for (int i = 0; i < N; ++i) t.insert(i, 0);
    double max_h = 2.0 * std::log2(N + 1);
    ASSERT_LE(t.height(), (int)max_h + 1);
    ASSERT(verify(t).ok);
}

// ── delete ─────────────────────────────────────────────────────────

TEST(rb_del_leaf, "RB: delete leaf") {
    bst::RBTree<int,int> t;
    t.insert(10,0); t.insert(5,0); t.insert(15,0);
    ASSERT(t.erase(5));
    ASSERT_EQ(t.find(5), nullptr);
    ASSERT(t.size() == 2);
    ASSERT(verify(t).ok);
}

TEST(rb_del_two_children, "RB: delete node with two children") {
    bst::RBTree<int,int> t;
    t.insert(10,0); t.insert(5,0); t.insert(15,0); t.insert(3,0); t.insert(7,0);
    ASSERT(t.erase(5));
    ASSERT_EQ(t.find(5), nullptr);
    ASSERT(t.size() == 4);
    ASSERT(verify(t).ok);
}

TEST(rb_del_root, "RB: delete root") {
    bst::RBTree<int,int> t;
    t.insert(10,0); t.insert(5,0); t.insert(15,0);
    ASSERT(t.erase(10));
    ASSERT_EQ(t.find(10), nullptr);
    ASSERT(verify(t).ok);
    if (t.root()) ASSERT_EQ((int)t.root()->color, (int)bst::Color::BLACK);
}

TEST(rb_del_red, "RB: delete red node (no fixup needed)") {
    bst::RBTree<int,int> t;
    // Build tree, find a red node, delete it
    for (int i = 0; i < 10; ++i) t.insert(i, 0);
    // Find a red node
    int red_key = -1;
    for (auto& n : t) {
        if (n.color == bst::Color::RED) { red_key = n.key; break; }
    }
    if (red_key >= 0) {
        ASSERT(t.erase(red_key));
        ASSERT(verify(t).ok);
    }
}

TEST(rb_del_sequential, "RB: sequential delete maintains properties") {
    bst::RBTree<int,int> t;
    for (int i = 0; i < 50; ++i) t.insert(i, 0);
    ASSERT(verify(t).ok);
    for (int i = 0; i < 50; ++i) {
        ASSERT(t.erase(i));
        auto r = verify(t);
        ASSERT_MSG(r.ok, r.msg);
    }
    ASSERT(t.empty());
}

// ── in-order ───────────────────────────────────────────────────────

TEST(rb_inorder, "RB: in-order traversal yields sorted keys") {
    bst::RBTree<int,int> t;
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

TEST(rb_succ_pred, "RB: successor and predecessor") {
    bst::RBTree<int,int> t;
    for (int i = 0; i < 10; ++i) t.insert(i*2, 0);
    auto s = t.successor(4);
    ASSERT(s && *s == 6);
    auto p = t.predecessor(4);
    ASSERT(p && *p == 2);
    ASSERT_EQ(t.successor(18), nullptr);
    ASSERT_EQ(t.predecessor(0), nullptr);
}

// ── duplicate key ──────────────────────────────────────────────────

TEST(rb_dup, "RB: duplicate key updates value") {
    bst::RBTree<int,int> t;
    t.insert(5, 10); t.insert(5, 20);
    ASSERT(t.size() == 1);
    ASSERT(t.find(5) && *t.find(5) == 20);
    ASSERT(verify(t).ok);
}

// ── empty tree ─────────────────────────────────────────────────────

TEST(rb_empty, "RB: empty tree operations") {
    bst::RBTree<int,int> t;
    ASSERT(t.empty());
    ASSERT_EQ(t.size(), 0u);
    ASSERT_EQ(t.height(), 0);
    ASSERT_EQ(t.find(1), nullptr);
    ASSERT_FALSE(t.erase(1));
}
