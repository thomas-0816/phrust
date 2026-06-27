<?php

$plain = function (int $x, int $y = 1): int {
    echo $x + $y;
};

$ref = static function & (array &$items) use (&$plain, $ref): mixed {
    echo $items[0];
};

$trailingUse = function () use (
    &$plain,
) {
    echo $plain(1);
};

$withAttribute = function (#[ParamAttr] string $value, ...$rest): void {
    echo $value;
};
