// test_main.cpp — Entry point that runs all test suites
#include "test_framework.hpp"

// Forward declarations from each test file
// test_construct.cpp
void test_default_construct(const std::string&);
void test_construct_from_zero(const std::string&);
void test_construct_positive(const std::string&);
void test_construct_negative(const std::string&);
void test_construct_int64_limits(const std::string&);
void test_construct_from_string(const std::string&);
void test_construct_from_hex(const std::string&);
void test_construct_invalid(const std::string&);
void test_copy_construct(const std::string&);
void test_even_odd(const std::string&);

// test_arithmetic.cpp
void test_add_basic(const std::string&);
void test_add_carry(const std::string&);
void test_add_large(const std::string&);
void test_sub_basic(const std::string&);
void test_sub_borrow(const std::string&);
void test_sub_result_negative(const std::string&);
void test_mul_basic(const std::string&);
void test_mul_large(const std::string&);
void test_mul_power_of_two(const std::string&);
void test_unary_neg(const std::string&);
void test_increment_decrement(const std::string&);
void test_mul_commutative(const std::string&);
void test_mul_associative_small(const std::string&);
void test_mul_distributive(const std::string&);

// test_comparison.cpp
void test_eq_basic(const std::string&);
void test_ne_basic(const std::string&);
void test_lt_basic(const std::string&);
void test_gt_basic(const std::string&);
void test_le_ge(const std::string&);
void test_cmp_large(const std::string&);
void test_cmp_cross_sign(const std::string&);

// test_division.cpp
void test_div_basic(const std::string&);
void test_div_large(const std::string&);
void test_div_exact(const std::string&);
void test_div_by_zero(const std::string&);
void test_mod_basic(const std::string&);
void test_mod_large(const std::string&);
void test_mod_by_zero(const std::string&);
void test_divmod_consistency(const std::string&);
void test_div_signs(const std::string&);
void test_div_multi_limb(const std::string&);
void test_div_single_limb_edge(const std::string&);

// test_string.cpp
void test_to_string_zero(const std::string&);
void test_to_string_basic(const std::string&);
void test_to_string_large(const std::string&);
void test_to_string_negative_large(const std::string&);
void test_hex_roundtrip(const std::string&);
void test_hex_basic(const std::string&);
void test_decimal_roundtrip(const std::string&);
void test_hex_power_of_two(const std::string&);
void test_string_edge_single_digit(const std::string&);

// test_karatsuba.cpp
void test_karatsuba_vs_schoolbook_small(const std::string&);
void test_karatsuba_vs_schoolbook_threshold(const std::string&);
void test_karatsuba_large_squares(const std::string&);
void test_karatsuba_different_sizes(const std::string&);
void test_karatsuba_associativity(const std::string&);

// test_math.cpp
void test_pow_basic(const std::string&);
void test_pow_large(const std::string&);
void test_pow_ten(const std::string&);
void test_modpow_basic(const std::string&);
void test_modpow_large(const std::string&);
void test_modpow_by_one(const std::string&);
void test_modpow_zero_exp(const std::string&);
void test_modpow_by_zero(const std::string&);
void test_gcd_basic(const std::string&);
void test_gcd_negative(const std::string&);
void test_gcd_large(const std::string&);

// test_signs.cpp
void test_sign_add_same_sign(const std::string&);
void test_sign_add_diff_sign(const std::string&);
void test_sign_sub(const std::string&);
void test_sign_mul(const std::string&);
void test_sign_div(const std::string&);
void test_sign_mod(const std::string&);
void test_sign_comparison(const std::string&);
void test_sign_unary_minus(const std::string&);
void test_sign_increment_zero(const std::string&);

int main() {
    std::cout << "BigInt Test Suite\n";

    // Construction tests
    RUN_TEST(test_default_construct);
    RUN_TEST(test_construct_from_zero);
    RUN_TEST(test_construct_positive);
    RUN_TEST(test_construct_negative);
    RUN_TEST(test_construct_int64_limits);
    RUN_TEST(test_construct_from_string);
    RUN_TEST(test_construct_from_hex);
    RUN_TEST(test_construct_invalid);
    RUN_TEST(test_copy_construct);
    RUN_TEST(test_even_odd);

    // Arithmetic tests
    RUN_TEST(test_add_basic);
    RUN_TEST(test_add_carry);
    RUN_TEST(test_add_large);
    RUN_TEST(test_sub_basic);
    RUN_TEST(test_sub_borrow);
    RUN_TEST(test_sub_result_negative);
    RUN_TEST(test_mul_basic);
    RUN_TEST(test_mul_large);
    RUN_TEST(test_mul_power_of_two);
    RUN_TEST(test_unary_neg);
    RUN_TEST(test_increment_decrement);
    RUN_TEST(test_mul_commutative);
    RUN_TEST(test_mul_associative_small);
    RUN_TEST(test_mul_distributive);

    // Comparison tests
    RUN_TEST(test_eq_basic);
    RUN_TEST(test_ne_basic);
    RUN_TEST(test_lt_basic);
    RUN_TEST(test_gt_basic);
    RUN_TEST(test_le_ge);
    RUN_TEST(test_cmp_large);
    RUN_TEST(test_cmp_cross_sign);

    // Division tests
    RUN_TEST(test_div_basic);
    RUN_TEST(test_div_large);
    RUN_TEST(test_div_exact);
    RUN_TEST(test_div_by_zero);
    RUN_TEST(test_mod_basic);
    RUN_TEST(test_mod_large);
    RUN_TEST(test_mod_by_zero);
    RUN_TEST(test_divmod_consistency);
    RUN_TEST(test_div_signs);
    RUN_TEST(test_div_multi_limb);
    RUN_TEST(test_div_single_limb_edge);

    // String conversion tests
    RUN_TEST(test_to_string_zero);
    RUN_TEST(test_to_string_basic);
    RUN_TEST(test_to_string_large);
    RUN_TEST(test_to_string_negative_large);
    RUN_TEST(test_hex_roundtrip);
    RUN_TEST(test_hex_basic);
    RUN_TEST(test_decimal_roundtrip);
    RUN_TEST(test_hex_power_of_two);
    RUN_TEST(test_string_edge_single_digit);

    // Karatsuba tests
    RUN_TEST(test_karatsuba_vs_schoolbook_small);
    RUN_TEST(test_karatsuba_vs_schoolbook_threshold);
    RUN_TEST(test_karatsuba_large_squares);
    RUN_TEST(test_karatsuba_different_sizes);
    RUN_TEST(test_karatsuba_associativity);

    // Math tests
    RUN_TEST(test_pow_basic);
    RUN_TEST(test_pow_large);
    RUN_TEST(test_pow_ten);
    RUN_TEST(test_modpow_basic);
    RUN_TEST(test_modpow_large);
    RUN_TEST(test_modpow_by_one);
    RUN_TEST(test_modpow_zero_exp);
    RUN_TEST(test_modpow_by_zero);
    RUN_TEST(test_gcd_basic);
    RUN_TEST(test_gcd_negative);
    RUN_TEST(test_gcd_large);

    // Sign tests
    RUN_TEST(test_sign_add_same_sign);
    RUN_TEST(test_sign_add_diff_sign);
    RUN_TEST(test_sign_sub);
    RUN_TEST(test_sign_mul);
    RUN_TEST(test_sign_div);
    RUN_TEST(test_sign_mod);
    RUN_TEST(test_sign_comparison);
    RUN_TEST(test_sign_unary_minus);
    RUN_TEST(test_sign_increment_zero);

    return test::run_all();
}
