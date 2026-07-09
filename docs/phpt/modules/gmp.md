# gmp PHPT Coverage

## Implemented scope

- Generated PHPTs cover BigInt-backed `gmp_init`, base parsing, string conversion, arithmetic, division, gcd/lcm/invert, roots, powm, bitwise operations, bit scans, population counts, factorial/binomial, import/export, and deterministic primality helpers.
- Global constants covered by the selected fixtures include rounding constants, word-order constants, endian constants, and `GMP_VERSION`.
- `gmp_jacobi`, `gmp_legendre`, and `gmp_kronecker` are implemented over a pure BigInt Kronecker-symbol helper.
- `gmp_random_seed` validates the seed and returns `NULL`; random value helpers remain deterministic in the selected facade.

## Remaining gaps

- Runtime GMP results are still string-backed values rather than native `GMP` object instances.
- `gmp_setbit` and `gmp_clrbit` require mutable `GMP` object support and are not enabled in selected coverage.
- Full GMP class serialization and object identity behavior are not implemented.
- Limb-order import/export flags and secure randomness still need broader compatibility work.
