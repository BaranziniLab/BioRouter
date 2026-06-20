/// @file test_stress.cpp  — Stress tests with thousands of random operations.
#include "test_framework.hpp"
#include "bst/bst.hpp"
#include "bst/avl.hpp"
#include "bst/rbtree.hpp"
#include "bst/verify.hpp"
#include <random>
#include <vector>
#include <algorithm>
#include <set>

using Comp = bst::DefaultComparator<int>;

static bst::VerifyResult verify_bst(const bst::BST<int,int>& t) {
    auto p = bst::verify_parents(t.root(),
                 static_cast<const bst::Node<int,int>*>(nullptr));
    if (!p.ok) return p;
    return bst::verify_bst_order(t.root(), Comp{});
}
static bst::VerifyResult verify_avl(const bst::AVL<int,int>& t) {
    auto p = bst::verify_parents(t.root(),
                 static_cast<const bst::Node<int,int>*>(nullptr));
    if (!p.ok) return p;
    return bst::verify_avl<int,int>(t.root(), Comp{});
}
static bst::VerifyResult verify_rb(const bst::RBTree<int,int>& t) {
    auto p = bst::verify_parents(t.root(),
                 static_cast<const bst::Node<int,int>*>(nullptr));
    if (!p.ok) return p;
    return bst::verify_rbtree<int,int>(t.root(), Comp{});
}

// ── BST stress: random insert/find ─────────────────────────────────

TEST(stress_bst_random, "Stress: BST random insert + find (5000 ops)") {
    std::mt19937 rng(42);
    std::uniform_int_distribution<int> dist(0, 4999);
    bst::BST<int,int> t;
    std::set<int> seen;
    const int N = 5000;
    for (int i = 0; i < N; ++i) {
        int k = dist(rng);
        t.insert(k, k * 10);
        seen.insert(k);
    }
    for (int k : seen) {
        ASSERT(t.find(k) != nullptr);
    }
    auto r = verify_bst(t);
    ASSERT_MSG(r.ok, r.msg);
}

// ── BST stress: random insert + delete ─────────────────────────────

TEST(stress_bst_mixed, "Stress: BST random insert/delete (5000 ops)") {
    std::mt19937 rng(123);
    std::uniform_int_distribution<int> dist(0, 999);
    bst::BST<int,int> t;
    std::set<int> present;
    for (int i = 0; i < 5000; ++i) {
        int k = dist(rng);
        if (i % 3 == 0 && !present.empty()) {
            // delete a random present key
            auto it = present.begin();
            std::advance(it, dist(rng) % present.size());
            t.erase(*it);
            present.erase(it);
        } else {
            t.insert(k, k);
            present.insert(k);
        }
    }
    for (int k : present) {
        ASSERT(t.find(k) != nullptr);
    }
    ASSERT_EQ(t.size(), present.size());
    auto r = verify_bst(t);
    ASSERT_MSG(r.ok, r.msg);
}

// ── AVL stress: random insert ──────────────────────────────────────

TEST(stress_avl_random, "Stress: AVL random insert (5000 ops)") {
    std::mt19937 rng(42);
    std::uniform_int_distribution<int> dist(0, 4999);
    bst::AVL<int,int> t;
    const int N = 5000;
    for (int i = 0; i < N; ++i) {
        t.insert(dist(rng), 0);
        auto r = verify_avl(t);
        ASSERT_MSG(r.ok, r.msg);
    }
    double max_h = 1.44 * std::log2(N + 2);
    ASSERT_LE(t.height(), (int)max_h + 1);
}

// ── AVL stress: random insert + delete ─────────────────────────────

TEST(stress_avl_mixed, "Stress: AVL random insert/delete (5000 ops)") {
    std::mt19937 rng(777);
    std::uniform_int_distribution<int> dist(0, 999);
    bst::AVL<int,int> t;
    std::set<int> present;
    for (int i = 0; i < 5000; ++i) {
        int k = dist(rng);
        if (i % 3 == 0 && !present.empty()) {
            auto it = present.begin();
            std::advance(it, dist(rng) % present.size());
            t.erase(*it);
            present.erase(it);
        } else {
            t.insert(k, k);
            present.insert(k);
        }
        auto r = verify_avl(t);
        ASSERT_MSG(r.ok, r.msg);
    }
    ASSERT_EQ(t.size(), present.size());
}

// ── AVL stress: sorted insert (worst-case for unbalanced) ──────────

TEST(stress_avl_sorted, "Stress: AVL sorted insert 1..5000") {
    bst::AVL<int,int> t;
    const int N = 5000;
    for (int i = 0; i < N; ++i) {
        t.insert(i, 0);
    }
    ASSERT_EQ(t.size(), (std::size_t)N);
    ASSERT_LE(t.height(), (int)(1.44 * std::log2(N + 2)) + 1);
    auto r = verify_avl(t);
    ASSERT_MSG(r.ok, r.msg);
}

// ── RBTree stress: random insert ───────────────────────────────────

TEST(stress_rb_random, "Stress: RB random insert (5000 ops)") {
    std::mt19937 rng(42);
    std::uniform_int_distribution<int> dist(0, 4999);
    bst::RBTree<int,int> t;
    const int N = 5000;
    for (int i = 0; i < N; ++i) {
        t.insert(dist(rng), 0);
        auto r = verify_rb(t);
        ASSERT_MSG(r.ok, r.msg);
    }
}

// ── RBTree stress: random insert + delete ──────────────────────────

TEST(stress_rb_mixed, "Stress: RB random insert/delete (5000 ops)") {
    std::mt19937 rng(999);
    std::uniform_int_distribution<int> dist(0, 999);
    bst::RBTree<int,int> t;
    std::set<int> present;
    for (int i = 0; i < 5000; ++i) {
        int k = dist(rng);
        if (i % 3 == 0 && !present.empty()) {
            auto it = present.begin();
            std::advance(it, dist(rng) % present.size());
            t.erase(*it);
            present.erase(it);
        } else {
            t.insert(k, k);
            present.insert(k);
        }
        auto r = verify_rb(t);
        ASSERT_MSG(r.ok, r.msg);
    }
    ASSERT_EQ(t.size(), present.size());
}

// ── RBTree stress: sorted insert (worst-case for unbalanced) ───────

TEST(stress_rb_sorted, "Stress: RB sorted insert 1..5000") {
    bst::RBTree<int,int> t;
    const int N = 5000;
    for (int i = 0; i < N; ++i) {
        t.insert(i, 0);
    }
    ASSERT_EQ(t.size(), (std::size_t)N);
    ASSERT_LE(t.height(), (int)(2.0 * std::log2(N + 1)) + 1);
    auto r = verify_rb(t);
    ASSERT_MSG(r.ok, r.msg);
}

// ── All three agree on find results ────────────────────────────────

TEST(stress_all_agree, "Stress: BST/AVL/RB agree on 2000 random lookups") {
    std::mt19937 rng(55);
    std::uniform_int_distribution<int> dist(0, 999);
    bst::BST<int,int>     bst_t;
    bst::AVL<int,int>     avl_t;
    bst::RBTree<int,int>  rb_t;

    for (int i = 0; i < 1000; ++i) {
        int k = dist(rng);
        bst_t.insert(k, k);
        avl_t.insert(k, k);
        rb_t.insert(k, k);
    }
    // every key found in one must be found in all, with same value
    for (int i = 0; i < 2000; ++i) {
        int k = dist(rng);
        auto* a = bst_t.find(k);
        auto* b = avl_t.find(k);
        auto* c = rb_t.find(k);
        ASSERT_EQ((a != nullptr), (b != nullptr));
        ASSERT_EQ((b != nullptr), (c != nullptr));
        if (a) {
            ASSERT_EQ(*a, *b);
            ASSERT_EQ(*b, *c);
        }
    }
}
