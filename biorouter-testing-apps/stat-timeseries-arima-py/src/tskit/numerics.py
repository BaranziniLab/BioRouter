"""Numeric utilities — thin wrappers that try numpy, fall back to pure Python."""

from __future__ import annotations

import math
import random
from typing import List, Sequence

# ---------------------------------------------------------------------------
# Optional numpy import
# ---------------------------------------------------------------------------
try:
    import numpy as np  # type: ignore

    HAS_NUMPY = True
except ImportError:
    np = None  # type: ignore
    HAS_NUMPY = False

# ---------------------------------------------------------------------------
# Type alias
# ---------------------------------------------------------------------------
Vector = List[float]


# ---------------------------------------------------------------------------
# Conversion helpers
# ---------------------------------------------------------------------------
def to_vec(x) -> Vector:
    """Ensure *x* is a plain Python list of floats."""
    if HAS_NUMPY and isinstance(x, np.ndarray):
        return [float(v) for v in x.tolist()]
    return [float(v) for v in x]


def zeros(n: int) -> Vector:
    return [0.0] * n


def ones(n: int) -> Vector:
    return [1.0] * n


# ---------------------------------------------------------------------------
# Linear algebra (pure-Python, good enough for moderate orders)
# ---------------------------------------------------------------------------
def dot(a: Vector, b: Vector) -> float:
    return sum(ai * bi for ai, bi in zip(a, b))


def mat_vec_mul(mat: List[Vector], vec: Vector) -> Vector:
    return [dot(row, vec) for row in mat]


def solve_toeplitz(r: Vector) -> Vector:
    """Solve T x = r where T is the Toeplitz matrix built from r[0..p].

    Uses Levinson-Durbin recursion (O(p^2)).  Returns coefficients
    a_1 … a_p (note: a_0 is implicitly 1).
    """
    p = len(r) - 1
    if p == 0:
        return []
    a = zeros(p)
    e = r[0]
    if abs(e) < 1e-30:
        return zeros(p)
    a[0] = r[1] / e
    for k in range(1, p):
        # Compute the reflection coefficient
        s = sum(a[j] * r[k - j] for j in range(k))
        lam = (r[k + 1] - s) / e
        a_new = zeros(p)
        a_new[k] = lam
        for j in range(k):
            a_new[j] = a[j] - lam * a[k - 1 - j]
        a = a_new
        e *= 1 - lam * lam
        if abs(e) < 1e-30:
            break
    return a


def cholesky_solve(A: List[Vector], b: Vector) -> Vector:
    """Solve A x = b where A is symmetric positive-definite, via Cholesky."""
    n = len(A)
    L = [zeros(n) for _ in range(n)]
    for i in range(n):
        for j in range(i + 1):
            s = sum(L[i][k] * L[j][k] for k in range(j))
            if i == j:
                val = A[i][i] - s
                L[i][j] = math.sqrt(max(val, 1e-30))
            else:
                L[i][j] = (A[i][j] - s) / L[j][j]
    # Forward substitution
    y = zeros(n)
    for i in range(n):
        y[i] = (b[i] - sum(L[i][k] * y[k] for k in range(i))) / L[i][i]
    # Back substitution
    x = zeros(n)
    for i in range(n - 1, -1, -1):
        x[i] = (y[i] - sum(L[k][i] * x[k] for k in range(i + 1, n))) / L[i][i]
    return x


def lstsq(A: List[Vector], b: Vector) -> Vector:
    """Least-squares solution to A x = b via normal equations A^T A x = A^T b."""
    n_cols = len(A[0])
    ATA = [[0.0] * n_cols for _ in range(n_cols)]
    ATb = [0.0] * n_cols
    for row_a, bi in zip(A, b):
        for j in range(n_cols):
            ATb[j] += row_a[j] * bi
            for k in range(n_cols):
                ATA[j][k] += row_a[j] * row_a[k]
    return cholesky_solve(ATA, ATb)


# ---------------------------------------------------------------------------
# Statistics helpers
# ---------------------------------------------------------------------------
def mean(x: Vector) -> float:
    return sum(x) / len(x) if x else 0.0


def variance(x: Vector, ddof: int = 0) -> float:
    n = len(x)
    if n <= ddof:
        return 0.0
    m = mean(x)
    return sum((xi - m) ** 2 for xi in x) / (n - ddof)


def std(x: Vector, ddof: int = 0) -> float:
    return math.sqrt(variance(x, ddof))


def cumsum(x: Vector) -> Vector:
    out = zeros(len(x))
    s = 0.0
    for i, v in enumerate(x):
        s += v
        out[i] = s
    return out


def diff(x: Vector, d: int = 1) -> Vector:
    """Apply d-th order differencing."""
    out = list(x)
    for _ in range(d):
        out = [out[i] - out[i - 1] for i in range(1, len(out))]
    return out


def undiff_order1(last_values: Vector, diffs: Vector) -> Vector:
    """Integrate: reconstruct series from initial value + first differences.

    undiff_order1([x_0], [d_1, ..., d_n]) → [x_0, x_0+d_1, x_0+d_1+d_2, ...]
    """
    x0 = last_values[0]
    result = [x0]
    for d in diffs:
        result.append(result[-1] + d)
    return result


def undiff(last_values: Vector, diffs: Vector) -> Vector:
    """Integrate d-th order differencing.

    For d=1: undiff([x_0], diffs) → [x_0, x_0+d_0, ...]  (length = len(diffs)+1)
    For d=2: undiff([x_0, Δ_1], diffs) → full series
       where Δ_1 = x_1 - x_0 (the first-order diff seed)

    *last_values* stores the seed values lost during differencing.
    For d=1, the seed is [x_0].
    For d=2, the seed is [x_0, x_1 - x_0].
    """
    d = len(last_values)
    out = list(diffs)
    for level in range(d):
        seed = last_values[d - 1 - level]
        temp = [seed]
        for v in out:
            temp.append(temp[-1] + v)
        out = temp
    return out


def arima_seeds(original: Vector, d: int) -> Vector:
    """Compute the seed values needed for undiff from the first d values of the series.

    For d=1: returns [x_0]
    For d=2: returns [x_0, x_1 - x_0]
    For d=3: returns [x_0, x_1 - x_0, x_2 - 2*x_1 + x_0]
    """
    if d == 0:
        return []
    seeds = [original[0]]
    y = list(original)
    for _ in range(d - 1):
        y = [y[i] - y[i - 1] for i in range(1, len(y))]
        seeds.append(y[0])
    return seeds


def seasonal_diff(x: Vector, m: int) -> Vector:
    return [x[i] - x[i - m] for i in range(m, len(x))]


def seasonal_undiff(last_vals: Vector, diffs: Vector, m: int) -> Vector:
    """Integrate seasonal differencing."""
    out = list(diffs)
    for i in range(len(out)):
        out[i] += last_vals[i % m]
    return out


# ---------------------------------------------------------------------------
# Random generation helpers (for tests)
# ---------------------------------------------------------------------------
_rng = random.Random(42)


def set_seed(s: int) -> None:
    _rng.seed(s)


def randn() -> float:
    """Box-Muller standard normal variate."""
    u1 = _rng.random()
    u2 = _rng.random()
    while u1 == 0:
        u1 = _rng.random()
    return math.sqrt(-2 * math.log(u1)) * math.cos(2 * math.pi * u2)


def randn_vec(n: int) -> Vector:
    return [randn() for _ in range(n)]


def simulate_ar(coeffs: Vector, n: int, sigma: float = 1.0) -> Vector:
    """Simulate AR(p) process: X_t = sum(a_i * X_{t-i}) + eps_t."""
    p = len(coeffs)
    x = zeros(n)
    for t in range(p, n):
        x[t] = sum(coeffs[i] * x[t - 1 - i] for i in range(p)) + sigma * randn()
    return x


def simulate_ma(coeffs: Vector, n: int, sigma: float = 1.0) -> Vector:
    """Simulate MA(q) process: X_t = eps_t + sum(b_i * eps_{t-i})."""
    q = len(coeffs)
    eps = [sigma * randn() for _ in range(n + q)]
    x = zeros(n)
    for t in range(n):
        x[t] = eps[t + q] + sum(coeffs[i] * eps[t + q - 1 - i] for i in range(q))
    return x


def simulate_arma(ar: Vector, ma: Vector, n: int, sigma: float = 1.0) -> Vector:
    """Simulate ARMA(p,q) via AR representation."""
    # Use long AR approximation
    maxlag = max(len(ar), len(ma)) * 4 + n
    eps = [sigma * randn() for _ in range(maxlag)]
    x = zeros(maxlag)
    p, q = len(ar), len(ma)
    for t in range(maxlag):
        ar_part = sum(ar[i] * x[t - 1 - i] for i in range(p) if t - 1 - i >= 0)
        ma_part = sum(ma[i] * eps[t - 1 - i] for i in range(q) if t - 1 - i >= 0)
        x[t] = ar_part + ma_part + eps[t]
    return x[maxlag - n:]


def acf(x: Vector, nlags: int = 40, d: int = 0) -> Vector:
    """Compute sample autocorrelation function."""
    y = diff(x, d) if d else list(x)
    n = len(y)
    m = mean(y)
    v = variance(y, ddof=0)
    if v == 0:
        return zeros(nlags + 1)
    result = [1.0]
    for k in range(1, nlags + 1):
        if k >= n:
            result.append(0.0)
        else:
            s = sum((y[t] - m) * (y[t - k] - m) for t in range(k, n))
            result.append(s / (n * v))
    return result


def pacf(x: Vector, nlags: int = 40) -> Vector:
    """Compute partial autocorrelation function via Durbin-Levinson."""
    r = acf(x, nlags)
    p = nlags
    phi = zeros(p + 1)
    phi_k = zeros(p + 1)
    phi_k[1] = r[1]
    pacf_vals = [1.0, r[1]]
    for k in range(2, p + 1):
        num = r[k] - sum(phi_k[j] * r[k - j] for j in range(1, k))
        den = 1.0 - sum(phi_k[j] * r[j] for j in range(1, k))
        if abs(den) < 1e-15:
            pacf_vals.append(0.0)
            continue
        phi_k_new = zeros(p + 1)
        phi_k_new[k] = num / den
        for j in range(1, k):
            phi_k_new[j] = phi_k[j] - phi_k_new[k] * phi_k[k - j]
        phi_k = phi_k_new
        pacf_vals.append(phi_k[k])
    return pacf_vals


def adf_test(x: Vector, maxlag: int | None = None) -> dict:
    """Augmented Dickey-Fuller test (no constant, no trend — simplified).

    Returns dict with 'statistic', 'lags', 'critical' values, and 'p_value' (approx).
    """
    n = len(x)
    if maxlag is None:
        maxlag = int(round(12 * (n / 100) ** 0.25))
    y = x
    dy = [y[i] - y[i - 1] for i in range(1, n)]
    # Build regression: dy_t = rho * y_{t-1} + sum(gamma_j * dy_{t-j}) + eps
    k = min(maxlag, n // 3)
    T = len(dy) - k
    if T <= k + 2:
        return {"statistic": 0.0, "lags": 0, "critical": {}, "p_value": 1.0}
    dep = []
    indep = []
    for t in range(k, len(dy)):
        dep.append(dy[t])
        row = [y[t]]  # y_{t} in the convention where dy_t = y_t - y_{t-1}
        for j in range(1, k + 1):
            row.append(dy[t - j])
        indep.append(row)
    # OLS
    ncol = len(indep[0])
    ATA = [[0.0] * ncol for _ in range(ncol)]
    ATb = [0.0] * ncol
    for row, bi in zip(indep, dep):
        for j in range(ncol):
            ATb[j] += row[j] * bi
            for kk in range(ncol):
                ATA[j][kk] += row[j] * row[kk]
    try:
        coeffs = cholesky_solve(ATA, ATb)
    except Exception:
        return {"statistic": 0.0, "lags": k, "critical": {}, "p_value": 1.0}
    rho = coeffs[0]
    # Residual variance
    resid_var = variance(
        [dep[i] - dot(indep[i], coeffs) for i in range(T)], ddof=ncol
    )
    # Standard error of rho
    try:
        inv_ATA = _inv(ATA)
        se_rho = math.sqrt(max(inv_ATA[0][0] * resid_var, 1e-30))
    except Exception:
        se_rho = 1.0
    adf_stat = rho / se_rho if se_rho > 0 else 0.0
    # MacKinnon approximate critical values (no constant, no trend)
    # These are rough approximations for n > ~100
    cv = {
        "1%": -2.58,
        "5%": -1.95,
        "10%": -1.62,
    }
    # Rough p-value approximation
    if adf_stat < -3.43:
        p = 0.01
    elif adf_stat < -2.86:
        p = 0.05
    elif adf_stat < -2.57:
        p = 0.10
    else:
        p = min(0.99, 0.5 * math.exp(0.5 * adf_stat))
    return {
        "statistic": adf_stat,
        "lags": k,
        "critical": cv,
        "p_value": p,
        "reject_5pct": adf_stat < cv["5%"],
    }


def _inv(A: List[Vector]) -> List[Vector]:
    """Invert a small SPD matrix via Cholesky."""
    n = len(A)
    L = [zeros(n) for _ in range(n)]
    for i in range(n):
        for j in range(i + 1):
            s = sum(L[i][k] * L[j][k] for k in range(j))
            if i == j:
                L[i][j] = math.sqrt(max(A[i][i] - s, 1e-30))
            else:
                L[i][j] = (A[i][j] - s) / L[j][j]
    inv_A = [zeros(n) for _ in range(n)]
    for col in range(n):
        e = zeros(n)
        e[col] = 1.0
        # Forward
        y = zeros(n)
        for i in range(n):
            y[i] = (e[i] - sum(L[i][k] * y[k] for k in range(i))) / L[i][i]
        # Back
        x = zeros(n)
        for i in range(n - 1, -1, -1):
            x[i] = (y[i] - sum(L[k][i] * x[k] for k in range(i + 1, n))) / L[i][i]
        for i in range(n):
            inv_A[i][col] = x[i]
    return inv_A
