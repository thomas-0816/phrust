<?php
function exact_divide($a, $b) { return $a / $b; }
function exact_modulo($a, $b) { return $a % $b; }
function exact_power($a, $b) { return $a ** $b; }
function exact_bit_and($a, $b) { return $a & $b; }
function exact_bit_or($a, $b) { return $a | $b; }
function exact_bit_xor($a, $b) { return $a ^ $b; }
function exact_shift_left($a, $b) { return $a << $b; }
function exact_shift_right($a, $b) { return $a >> $b; }

foreach ([12, '12'] as $left) {
    echo exact_divide($left, 2), '|', exact_modulo($left, 2), '|',
        exact_power($left, 2), '|', exact_bit_and($left, 2), '|',
        exact_bit_or($left, 2), '|', exact_bit_xor($left, 2), '|',
        exact_shift_left($left, 2), '|', exact_shift_right($left, 2), "\n";
}

try { exact_divide(1, 0); } catch (Throwable $error) {
    echo get_class($error), ':', $error->getMessage(), "\n";
}
try { exact_modulo(1, 0); } catch (Throwable $error) {
    echo get_class($error), ':', $error->getMessage(), "\n";
}
try { exact_shift_left(1, -1); } catch (Throwable $error) {
    echo get_class($error), ':', $error->getMessage(), "\n";
}
