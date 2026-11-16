# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# The numerical checks run while docs/MATH.md was written (ADAA-1/2 closed forms,
# the ADAA-2 fallback, the feedback beta table, the Kapteyn DFT against mpmath,
# Kapteyn truncation, the EG closed forms and totals, the droop figures).
# Exploratory: M1 replaces it with reference-vector generators that ruff checks.
# Needs numpy, scipy and mpmath.

import numpy as np, mpmath as mp
mp.mp.dps = 40
rng = np.random.default_rng(1)
def sinc(u): return 1.0 if u == 0 else np.sin(np.pi*u)/(np.pi*u)
def sincp(u): return 0.0 if u == 0 else (np.cos(np.pi*u) - sinc(u))/u

# ADAA-1 closed form vs integral
err1 = 0
for _ in range(2000):
    x0 = rng.uniform(-3, 3); d = rng.uniform(-0.6, 0.6)
    exact = float(mp.quad(lambda t: mp.sin(2*mp.pi*(x0 + t*d)), [0, 1]))
    closed = sinc(d)*np.sin(2*np.pi*(x0 + d - d/2))
    err1 = max(err1, abs(exact - closed))
print("ADAA-1 max err", err1)

# ADAA-2: closed-form divided differences vs triangular-kernel integral of linear interpolation
F2 = lambda a, b: -np.cos(np.pi*(a+b))*sinc(a-b)/(2*np.pi)
def adaa2(x2, x1, x0):  # x2 = x_n, x1 = x_{n-1}, x0 = x_{n-2}
    return 2*(F2(x2, x1) - F2(x1, x0))/(x2 - x0)
def adaa2_deriv(x2, x1, x0):
    a = (x2 + x0)/2; b = x1
    return 2*(0.5*np.sin(np.pi*(a+b))*sinc(a-b) - np.cos(np.pi*(a+b))*sincp(a-b)/(2*np.pi))
def adaa2_exact(x2, x1, x0):
    # triangular kernel over [n-2, n] centred at n-1 applied to piecewise-linear x(t)
    f = lambda t: mp.sin(2*mp.pi*(x0 + t*(x1 - x0))) * t
    g = lambda t: mp.sin(2*mp.pi*(x1 + t*(x2 - x1))) * (1 - t)
    return float(mp.quad(f, [0, 1]) + mp.quad(g, [0, 1]))
err2 = 0
for _ in range(800):
    x0 = rng.uniform(-2, 2); x1 = x0 + rng.uniform(-0.4, 0.4); x2 = x1 + rng.uniform(-0.4, 0.4)
    if abs(x2 - x0) < 1e-3: continue
    err2 = max(err2, abs(adaa2(x2, x1, x0) - adaa2_exact(x2, x1, x0)))
print("ADAA-2 max err vs triangular kernel", err2)
# derivative branch near x_n == x_{n-2}
errd = 0
for _ in range(400):
    x0 = rng.uniform(-2, 2); x1 = x0 + rng.uniform(-0.4, 0.4); x2 = x0 + rng.uniform(-1e-6, 1e-6)
    errd = max(errd, abs(adaa2_deriv(x2, x1, x0) - adaa2_exact(x2, x1, x0)))
print("ADAA-2 derivative-branch max err (|x_n-x_{n-2}|<1e-6)", errd)
# equal points
print("equal points ->", adaa2_deriv(0.1, 0.1, 0.1), np.sin(2*np.pi*0.1))

# Feedback beta table
print("beta at g=2:", [round(2*np.pi*2**(fb-8)*2, 3) for fb in range(1, 8)])

# Kepler / Kapteyn: series vs Newton for beta < 1
def kepler(phi, beta):
    th = phi + beta*np.sin(phi)
    for _ in range(50):
        h = th - beta*np.sin(th) - phi
        th -= h/(1 - beta*np.cos(th))
    return th
for beta in (0.3, 0.785, 0.95):
    K = 4096
    ph = 2*np.pi*np.arange(K)/K
    s = np.sin([kepler(p, beta) for p in ph])
    a = 2*np.fft.rfft(s).imag*(-1)/K   # sine coefficients
    # compare with Bessel formula for first 10
    bess = [float(2/(n*beta)*mp.besselj(n, n*beta)) for n in range(1, 11)]
    print(f"beta={beta}: max |DFT - Bessel| n<=10:", max(abs(a[n] - bess[n-1]) for n in range(1, 11)))
    Om = beta*np.exp(np.sqrt(1-beta**2))/(1+np.sqrt(1-beta**2))
    nk = next(n for n in range(1, 100000) if 2/(n*beta)*Om**n/(1-Om) < 2**-53)
    print(f"   Omega={Om:.4f}  N_K={nk}  |a_N_K| actual={abs(float(2/(nk*beta)*mp.besselj(nk, nk*beta))):.2e}")

# EG closed forms
Tfull = 3840.0; Tstar = Tfull + 512
C = 49096/4096
def rho(q): return C*2**np.floor(q/4)*(1 + (q - 4*np.floor(q/4))/4)
print("rho(0) dB/s =", rho(0)*20*np.log10(2)/256, " (Levien 0.2819)")
# check ODE solution numerically
from scipy.integrate import solve_ivp
q = 40; r = rho(q); T0 = 1716.0
sol = solve_ivp(lambda t, T: r*(2 + (Tfull - T)/256), [0, 0.05], [T0], rtol=1e-12, atol=1e-12, dense_output=True)
tt = np.linspace(0, 0.05, 50)
closed = Tstar - (Tstar - T0)*np.exp(-r*tt/256)
print("attack closed-form vs ODE max err:", np.max(np.abs(sol.sol(tt)[0] - closed)))
# T totals
s = lambda L: 28 + L if L >= 20 else [0,5,9,13,17,20,23,25,27,29,31,33,35,37,39,41,42,43,45,46][L]
print("T_full =", 64*(s(99)//2) + 32*s(99) - 4256)
print("max reachable T =", 64*(s(99)//2) + 32*127 + 224 - 4256, "< T* =", Tstar)
# ADAA droop numbers
for fe in (44100, 48000, 96000):
    print(fe, "sinc dB @20k:", round(20*np.log10(sinc(20000/fe)), 2), "sinc^2:", round(40*np.log10(sinc(20000/fe)), 2))
# end gain
print("g at -200 dB:", 10**(-200/20), "log2", np.log2(10**(-200/20)))

print("--- ADAA-2 linear-input check")
e = 0
for _ in range(300):
    x0 = rng.uniform(-2, 2); d = rng.uniform(-0.4, 0.4)
    if abs(d) < 1e-3: continue
    x1 = x0 + d; x2 = x1 + d
    e = max(e, abs(adaa2(x2, x1, x0) - adaa2_exact(x2, x1, x0)))
print("linear x: max err", e)
# direct Bilbao formula with F2 evaluated pointwise (no closed form) to rule out algebra slip
F2p = lambda x: -np.sin(2*np.pi*x)/(4*np.pi**2)
def bilbao(x2, x1, x0):
    return 2/(x2 - x0)*((F2p(x2) - F2p(x1))/(x2 - x1) - (F2p(x1) - F2p(x0))/(x1 - x0))
e = 0
for _ in range(300):
    x0 = rng.uniform(-2, 2); x1 = x0 + rng.uniform(-0.4, 0.4); x2 = x1 + rng.uniform(-0.4, 0.4)
    if min(abs(x2-x0), abs(x2-x1), abs(x1-x0)) < 1e-3: continue
    e = max(e, abs(adaa2(x2, x1, x0) - bilbao(x2, x1, x0)))
print("closed form vs pointwise Bilbao formula:", e)

print("--- ADAA-2 fallback branch vs high-precision Bilbao")
def bilbao_mp(x2, x1, x0):
    x2, x1, x0 = mp.mpf(x2), mp.mpf(x1), mp.mpf(x0)
    F = lambda x: -mp.sin(2*mp.pi*x)/(4*mp.pi**2)
    return float(2/(x2 - x0)*((F(x2) - F(x1))/(x2 - x1) - (F(x1) - F(x0))/(x1 - x0)))
for h in (1e-4, 1e-5, 2**-17, 1e-6, 1e-8):
    e_d = e_c = 0
    for _ in range(200):
        x0 = rng.uniform(-2, 2); x1 = x0 + rng.uniform(0.01, 0.4)*rng.choice([-1, 1]); x2 = x0 + h*rng.choice([-1, 1])
        ref = bilbao_mp(x2, x1, x0)
        e_d = max(e_d, abs(adaa2_deriv(x2, x1, x0) - ref))
        e_c = max(e_c, abs(adaa2(x2, x1, x0) - ref))
    print(f"|x_n-x_(n-2)|={h:.1e}: derivative branch err {e_d:.2e}, closed-form quotient err {e_c:.2e}")

print("--- ADAA-2 fallback with the h^2/24 third-derivative correction")
def F2ab_mp(a, b):
    a, b = mp.mpf(a), mp.mpf(b)
    return (-(mp.sin(2*mp.pi*a) - mp.sin(2*mp.pi*b))/(4*mp.pi**2))/(a - b)
for h in (1e-2, 1e-3, 2**-10, 1e-4):
    e3 = 0; eq = 0
    for _ in range(100):
        x0 = rng.uniform(-2, 2); x1 = x0 + rng.uniform(0.01, 0.4)*rng.choice([-1, 1]); x2 = x0 + h*rng.choice([-1, 1])
        ref = bilbao_mp(x2, x1, x0)
        a = (x2 + x0)/2; hh = x2 - x0
        d1 = adaa2_deriv(x2, x1, x0)/2
        d3 = float(mp.diff(lambda s: F2ab_mp(s, x1), a, 3))
        e3 = max(e3, abs(2*(d1 + hh**2/24*d3) - ref))
        eq = max(eq, abs(adaa2(x2, x1, x0) - ref))
    print(f"h={h:.1e}: corrected expansion err {e3:.2e}, closed-form quotient err {eq:.2e}")
