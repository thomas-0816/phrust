<?php

function external_native_catch_boundary($value): int {
    try {
        return sizeof($value);
    } catch (TypeError $error) {
        return 77;
    }
}
