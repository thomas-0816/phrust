<?php
// runtime-semantics: category=types expect=pass
declare(strict_types=1);

function add_one(int $value): int {
    return $value + 1;
}

echo add_one("41"), "\n";
