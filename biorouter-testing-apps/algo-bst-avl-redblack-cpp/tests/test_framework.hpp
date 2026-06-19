#pragma once
/// @file test_framework.hpp
/// Minimal assertion-based test framework (no external dependencies).

#include <iostream>
#include <string>
#include <vector>
#include <functional>
#include <stdexcept>

namespace test {

struct TestCase {
    std::string name;           // display name, e.g. "BST: insert basic"
    std::function<void()> func;
};

inline std::vector<TestCase>& registry() {
    static std::vector<TestCase> r;
    return r;
}

inline int total_passed  = 0;
inline int total_failed  = 0;
inline int suite_passed  = 0;
inline int suite_failed  = 0;

inline void begin_suite(const std::string& name) {
    suite_passed = 0;
    suite_failed = 0;
    std::cout << "\n-- " << name << " " << std::string(54 - std::min(name.size(), size_t(50)), '-') << "\n";
}

inline void end_suite() {
    std::cout << "   " << suite_passed << " passed, " << suite_failed << " failed\n";
    total_passed += suite_passed;
    total_failed += suite_failed;
}

inline int register_test(const char* name, std::function<void()> func) {
    registry().push_back({name, std::move(func)});
    return 0;
}

inline int run_all() {
    std::string last_suite;
    for (auto& tc : registry()) {
        auto pos = tc.name.find(':');
        std::string suite = (pos != std::string::npos) ? tc.name.substr(0, pos) : "misc";
        if (suite != last_suite) {
            if (!last_suite.empty()) end_suite();
            begin_suite(suite);
            last_suite = suite;
        }
        try {
            tc.func();
            std::cout << "   PASS " << tc.name << "\n";
            ++suite_passed;
        } catch (const std::exception& e) {
            std::cout << "   FAIL " << tc.name << "\n     " << e.what() << "\n";
            ++suite_failed;
        }
    }
    if (!last_suite.empty()) end_suite();
    std::cout << "\n========================================\n";
    std::cout << "  TOTAL: " << total_passed << " passed, "
              << total_failed << " failed\n";
    std::cout << "========================================\n";
    return total_failed;
}

} // namespace test

// ── macros ─────────────────────────────────────────────────────────

/// TEST(unique_id, "Suite: descriptive name") { body }
#define TEST(id, display_name)                                              \
    static void test_fn_##id();                                             \
    static int reg_##id = ::test::register_test(display_name, test_fn_##id);\
    static void test_fn_##id()

#define ASSERT(cond)                                                        \
    do {                                                                    \
        if (!(cond))                                                        \
            throw std::runtime_error(                                       \
                std::string("ASSERT failed: ") + #cond                      \
                + "  [" __FILE__ ":" + std::to_string(__LINE__) + "]");     \
    } while (0)

#define ASSERT_EQ(a, b)  ASSERT((a) == (b))
#define ASSERT_NE(a, b)  ASSERT((a) != (b))
#define ASSERT_TRUE(c)   ASSERT(c)
#define ASSERT_FALSE(c)  ASSERT(!(c))
#define ASSERT_GT(a, b)  ASSERT((a) > (b))
#define ASSERT_LT(a, b)  ASSERT((a) < (b))
#define ASSERT_GE(a, b)  ASSERT((a) >= (b))
#define ASSERT_LE(a, b)  ASSERT((a) <= (b))

#define ASSERT_MSG(cond, msg)                                               \
    do {                                                                    \
        if (!(cond))                                                        \
            throw std::runtime_error(                                       \
                std::string("ASSERT failed: ") + #cond                      \
                + " - " + std::string(msg)                                  \
                + "  [" __FILE__ ":" + std::to_string(__LINE__) + "]");     \
    } while (0)
