# bcmath PHPT Coverage

## Implemented scope

- Generated PHPTs cover `bcadd`, `bcsub`, `bcmul`, `bcdiv`, `bcmod`, `bcpow`, `bcpowmod`, `bcsqrt`, `bccomp`, and `bcscale`.
- Decimal arithmetic is backed by `num-bigint` integer units with explicit scale truncation.
- `bcscale()` is request-local and persists across VM builtin calls.
- `bcpowmod()` accepts decimal integer strings with zero fractional tails, rejects non-zero fractional tails, handles negative bases, and scales the result.
- `bcsqrt()` truncates to the requested scale and normalizes negative zero.

## Remaining gaps

- `BcMath\Number` object API is not implemented.
- `bcdivmod`, `bcfloor`, `bcceil`, and `bcround` are not part of the selected generated coverage yet.
- Full php-src warning and rounding-edge parity still needs broader corpus triage.
