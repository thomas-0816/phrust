<?php

$checks = [
    acos(1.0) === 0.0,
    acosh(1.0) === 0.0,
    asin(0.0) === 0.0,
    asinh(0.0) === 0.0,
    atan(0.0) === 0.0,
    atan2(0.0, 1.0) === 0.0,
    atanh(0.0) === 0.0,
    cos(0.0) === 1.0,
    cosh(0.0) === 1.0,
    deg2rad(180.0) === pi(),
    exp(0.0) === 1.0,
    expm1(0.0) === 0.0,
    fpow(3.0, 2.0) === 9.0,
    hypot(3.0, 4.0) === 5.0,
    log(1.0) === 0.0,
    log10(1.0) === 0.0,
    log1p(0.0) === 0.0,
    rad2deg(pi()) === 180.0,
    sin(0.0) === 0.0,
    sinh(0.0) === 0.0,
    tan(0.0) === 0.0,
    tanh(0.0) === 0.0,
];

foreach ($checks as $check) {
    echo $check ? '1' : '0';
}
echo "\n";

// Numeric strings preserve PHP conversion semantics through the operation's
// one baseline-native continuation; ordinary numeric calls above stay native.
echo sin("0"), "\n";
