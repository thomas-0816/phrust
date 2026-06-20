<?php

function scalar_types(
    int $id,
    ?string $name,
    array|callable|null $handler,
    iterable $items,
    object $object,
    mixed $value,
    false|null $flag
): Foo|Bar|null {
    echo $id;
}

function by_reference(Foo\Bar &$value): \Vendor\Type {
    echo $value;
}

function bottom(): never {
    echo "stop";
}

class TypeBase {}

class TypeContext extends TypeBase {
    public function method(self|parent|null $value): static {
        echo "method";
    }
}
