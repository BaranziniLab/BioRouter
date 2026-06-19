/// @file benchmark.cpp  — Performance comparison of BST vs AVL vs Red-Black tree.
#include "bst/bst.hpp"
#include "bst/avl.hpp"
#include "bst/rbtree.hpp"
#include <chrono>
#include <random>
#include <vector>
#include <algorithm>
#include <iostream>
#include <iomanip>
#include <numeric>

using Clock = std::chrono::high_resolution_clock;
using Ms    = std::chrono::duration<double, std::milli>;

static const int N = 50000;

template <typename Fn>
double time_ms(Fn fn) {
    auto t0 = Clock::now();
    fn();
    return std::chrono::duration_cast<Ms>(Clock::now() - t0).count();
}

int main() {
    // Pre-generate data
    std::mt19937 rng(42);
    std::vector<int> random_keys(N);
    std::iota(random_keys.begin(), random_keys.end(), 0);
    std::shuffle(random_keys.begin(), random_keys.end(), rng);

    std::vector<int> sorted_keys(N);
    std::iota(sorted_keys.begin(), sorted_keys.end(), 0);

    // Random lookups (from the set we inserted)
    std::vector<int> lookup_keys(N);
    for (int i = 0; i < N; ++i) lookup_keys[i] = random_keys[rng() % N];

    std::cout << "=== BST / AVL / Red-Black Tree Benchmark ===\n";
    std::cout << "  N = " << N << "\n\n";

    auto run = [&](const char* label,
                   auto make_tree,
                   const std::vector<int>& insert_keys,
                   const std::vector<int>* find_keys) {
        std::cout << label << ":\n";

        auto t_bst = time_ms([&]{
            auto t = make_tree.template operator()<bst::BST<int,int>>();
            for (int k : insert_keys) t->insert(k, k);
            if (find_keys) for (int k : *find_keys) (void)t->find(k);
        });
        auto t_avl = time_ms([&]{
            auto t = make_tree.template operator()<bst::AVL<int,int>>();
            for (int k : insert_keys) t->insert(k, k);
            if (find_keys) for (int k : *find_keys) (void)t->find(k);
        });
        auto t_rb = time_ms([&]{
            auto t = make_tree.template operator()<bst::RBTree<int,int>>();
            for (int k : insert_keys) t->insert(k, k);
            if (find_keys) for (int k : *find_keys) (void)t->find(k);
        });

        std::cout << "  BST:       " << std::fixed << std::setprecision(2) << t_bst << " ms\n";
        std::cout << "  AVL:       " << t_avl << " ms\n";
        std::cout << "  Red-Black: " << t_rb << " ms\n\n";
    };

    // ── Random insertion ──────────────────────────────────────────
    {
        std::cout << "Random insertion (N=" << N << "):\n";
        auto t_bst = time_ms([&]{
            bst::BST<int,int> t;
            for (int k : random_keys) t.insert(k, k);
        });
        auto t_avl = time_ms([&]{
            bst::AVL<int,int> t;
            for (int k : random_keys) t.insert(k, k);
        });
        auto t_rb = time_ms([&]{
            bst::RBTree<int,int> t;
            for (int k : random_keys) t.insert(k, k);
        });
        std::cout << "  BST:       " << std::fixed << std::setprecision(2) << t_bst << " ms\n";
        std::cout << "  AVL:       " << t_avl << " ms\n";
        std::cout << "  Red-Black: " << t_rb << " ms\n\n";
    }

    // ── Sorted insertion (worst case for unbalanced BST) ──────────
    {
        std::cout << "Sorted insertion (N=" << N << "):\n";
        auto t_bst = time_ms([&]{
            bst::BST<int,int> t;
            for (int k : sorted_keys) t.insert(k, k);
        });
        auto t_avl = time_ms([&]{
            bst::AVL<int,int> t;
            for (int k : sorted_keys) t.insert(k, k);
        });
        auto t_rb = time_ms([&]{
            bst::RBTree<int,int> t;
            for (int k : sorted_keys) t.insert(k, k);
        });
        std::cout << "  BST:       " << std::fixed << std::setprecision(2) << t_bst << " ms\n";
        std::cout << "  AVL:       " << t_avl << " ms\n";
        std::cout << "  Red-Black: " << t_rb << " ms\n\n";
    }

    // ── Random lookup (from randomly-built tree) ──────────────────
    {
        // Build trees
        bst::BST<int,int>    bst_t;
        bst::AVL<int,int>    avl_t;
        bst::RBTree<int,int> rb_t;
        for (int k : random_keys) { bst_t.insert(k, k); avl_t.insert(k, k); rb_t.insert(k, k); }

        std::cout << "Random lookup (N=" << N << ", " << N << " lookups):\n";
        auto t_bst = time_ms([&]{ for (int k : lookup_keys) (void)bst_t.find(k); });
        auto t_avl = time_ms([&]{ for (int k : lookup_keys) (void)avl_t.find(k); });
        auto t_rb  = time_ms([&]{ for (int k : lookup_keys) (void)rb_t.find(k); });
        std::cout << "  BST:       " << std::fixed << std::setprecision(2) << t_bst << " ms\n";
        std::cout << "  AVL:       " << t_avl << " ms\n";
        std::cout << "  Red-Black: " << t_rb << " ms\n\n";
    }

    // ── Sorted lookup (from sorted-built tree) ────────────────────
    {
        bst::BST<int,int>    bst_t;
        bst::AVL<int,int>    avl_t;
        bst::RBTree<int,int> rb_t;
        for (int k : sorted_keys) { bst_t.insert(k, k); avl_t.insert(k, k); rb_t.insert(k, k); }

        std::cout << "Sorted-lookup (from sorted-insertion tree, " << N << " lookups):\n";
        auto t_bst = time_ms([&]{ for (int k : lookup_keys) (void)bst_t.find(k); });
        auto t_avl = time_ms([&]{ for (int k : lookup_keys) (void)avl_t.find(k); });
        auto t_rb  = time_ms([&]{ for (int k : lookup_keys) (void)rb_t.find(k); });
        std::cout << "  BST:       " << std::fixed << std::setprecision(2) << t_bst << " ms\n";
        std::cout << "  AVL:       " << t_avl << " ms\n";
        std::cout << "  Red-Black: " << t_rb << " ms\n\n";
    }

    std::cout << "Done.\n";
    return 0;
}
