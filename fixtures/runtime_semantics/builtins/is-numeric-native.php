<?php
function native_is_numeric($value): bool {
    return is_numeric($value);
}

foreach ([
    42,
    1.5,
    true,
    null,
    "  +1.5e2 \f",
    ".5",
    "1.",
    "1e",
    "+",
    "1.5x",
    "",
] as $value) {
    var_dump(native_is_numeric($value));
}
