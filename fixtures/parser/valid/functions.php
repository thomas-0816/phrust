<?php

function greet(string $name = "world"): void {
    echo $name;
}

function &identity(array &$items, mixed ...$rest): mixed {
    echo $items[0];
}

function annotated(string $name, #[ParamAttr] ?int $count = null): string|null {
    echo $name;
}
