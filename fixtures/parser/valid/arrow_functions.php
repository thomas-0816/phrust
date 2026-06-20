<?php

$double = fn(int $x): int => $x * 2;
$withDefault = static fn (?string $value = null): string|null => $value ?? "fallback";
$withAttribute = fn (#[ParamAttr] array $items): mixed => $items[0] ?? null;
