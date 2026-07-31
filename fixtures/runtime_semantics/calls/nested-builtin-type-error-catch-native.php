<?php

function scalar_sizeof(): int
{
    try {
        return sizeof(42);
    } catch (TypeError $error) {
        return 77;
    }
}

function scalar_count(): int
{
    try {
        return count(null);
    } catch (TypeError $error) {
        return 88;
    }
}

var_dump(scalar_sizeof(), scalar_count());
