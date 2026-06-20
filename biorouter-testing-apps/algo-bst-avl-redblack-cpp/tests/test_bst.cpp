/// @file test_bst.cpp  — Unit tests for the unbalanced BST.
#include "test_framework.hpp"
#include "bst/bst.hpp"
#include "bst/verify.hpp"

using Comp = bst::DefaultComparator<int>;

static bst::VerifyResult verify(const bst::BST<int,int>& t) {
    return bst::verify_bst_order(t.root(), Comp{});
}
static bst::VerifyResult verify_p(const bst::BST<int,int>& t) {
    return bst::verify_parents(t.root(), static_cast<const bst::Node<int,int>*>(nullptr));
}

// ── insert / find ──────────────────────────────────────────────────

TEST(bst_insert_find, "BST: insert and find") {
    bst::BST<int,int> t;
    t.insert(5, 50); t.insert(3, 30); t.insert(7, 70);
    ASSERT(t.find(5) && *t.find(5) == 50);
    ASSERT(t.find(3) && *t.find(3) == 30);
    ASSERT(t.find(7) && *t.find(7) == 70);
    ASSERT_EQ(t.find(99), nullptr);
    ASSERT(t.size() == 3);
    ASSERT(verify(t).ok);
    ASSERT(verify_p(t).ok);
}

TEST(bst_dup_update, "BST: duplicate key updates value") {
    bst::BST<int,int> t;
    t.insert(1, 10); t.insert(1, 20);
    ASSERT(t.size() == 1);
    ASSERT(t.find(1) && *t.find(1) == 20);
}

TEST(bst_empty, "BST: empty tree operations") {
    bst::BST<int,int> t;
    ASSERT(t.empty());
    ASSERT_EQ(t.size(), 0u);
    ASSERT_EQ(t.height(), 0);
    ASSERT_EQ(t.find(1), nullptr);
}

// ── min / max ──────────────────────────────────────────────────────

TEST(bst_min_max, "BST: min and max") {
    bst::BST<int,int> t;
    t.insert(5,0); t.insert(2,0); t.insert(8,0); t.insert(1,0); t.insert(9,0);
    ASSERT_EQ(t.min_key(), 1);
    ASSERT_EQ(t.max_key(), 9);
    ASSERT(verify(t).ok);
}

// ── successor / predecessor ────────────────────────────────────────

TEST(bst_succ_pred, "BST: successor and predecessor") {
    bst::BST<int,int> t;
    for (int i = 0; i < 10; ++i) t.insert(i, 0);  // degenerates to a chain
    // find successor of 4
    auto s = t.successor(4);
    ASSERT(s && *s == 5);
    auto p = t.predecessor(4);
    ASSERT(p && *p == 3);
    // no successor of max
    ASSERT_EQ(t.successor(9), nullptr);
    // no predecessor of min
    ASSERT_EQ(t.predecessor(0), nullptr);
}

// ── delete ─────────────────────────────────────────────────────────

TEST(bst_del_leaf, "BST: delete leaf") {
    bst::BST<int,int> t;
    t.insert(5,0); t.insert(3,0); t.insert(7,0);
    ASSERT(t.erase(3));
    ASSERT_EQ(t.find(3), nullptr);
    ASSERT(t.size() == 2);
    ASSERT(verify(t).ok);
    ASSERT(verify_p(t).ok);
}

TEST(bst_del_one_child, "BST: delete node with one child") {
    bst::BST<int,int> t;
    t.insert(5,0); t.insert(3,0); t.insert(2,0);
    ASSERT(t.erase(3));
    ASSERT(t.find(2) && t.find(5));
    ASSERT(t.size() == 2);
    ASSERT(verify(t).ok);
    ASSERT(verify_p(t).ok);
}

TEST(bst_del_two_children, "BST: delete node with two children") {
    bst::BST<int,int> t;
    t.insert(5,0); t.insert(3,0); t.insert(7,0); t.insert(6,0); t.insert(8,0);
    ASSERT(t.erase(7));
    ASSERT_EQ(t.find(7), nullptr);
    ASSERT(t.find(6) && t.find(8));
    ASSERT(t.size() == 4);
    ASSERT(verify(t).ok);
    ASSERT(verify_p(t).ok);
}

TEST(bst_del_root, "BST: delete root") {
    bst::BST<int,int> t;
    t.insert(5,0); t.insert(3,0); t.insert(7,0);
    ASSERT(t.erase(5));
    ASSERT_EQ(t.find(5), nullptr);
    ASSERT(t.size() == 2);
    ASSERT(verify(t).ok);
    ASSERT(verify_p(t).ok);
}

TEST(bst_del_nonexistent, "BST: delete nonexistent key") {
    bst::BST<int,int> t;
    t.insert(5,0);
    ASSERT_FALSE(t.erase(99));
    ASSERT(t.size() == 1);
}

// ── in-order traversal ─────────────────────────────────────────────

TEST(bst_inorder, "BST: in-order traversal yields sorted keys") {
    bst::BST<int,int> t;
    int keys[] = {5, 3, 7, 1, 4, 6, 8};
    for (int k : keys) t.insert(k, 0);
    int prev = -1;
    for (auto& n : t) {
        ASSERT_GT(n.key, prev);
        prev = n.key;
    }
    ASSERT(verify(t).ok);
}

// ── height / size ──────────────────────────────────────────────────

TEST(bst_height_size, "BST: height and size") {
    bst::BST<int,int> t;
    ASSERT_EQ(t.height(), 0);
    t.insert(5,0);
    ASSERT_EQ(t.height(), 1);
    t.insert(3,0); t.insert(7,0);
    ASSERT_EQ(t.height(), 2);
    ASSERT_EQ(t.size(), 3u);
}
