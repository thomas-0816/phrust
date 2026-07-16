<?php

function lazy_cross_unit_nested_method_callback($value) {
    // Action callbacks commonly return implicitly; apply_filters() must still
    // replace its private accumulator without reviving a released handle.
}
