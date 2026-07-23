<?php
// Typed static properties keep their type guard when the storage is exposed
// through a PHP reference. This covers uninitialized reads, by-value call
// transport, by-reference initialization, weak coercion, rejected writes,
// and array auto-initialization through the same reference identity.
class TypedStaticReference {
    public static ?int $number;
    public static ?array $items;
}

function read_ref(&$target): void {
    var_dump($target);
}

function write_ref(&$target, $value): void {
    $target = $value;
}

function append_ref(&$target): void {
    $target[] = 1;
}

try {
    var_dump(TypedStaticReference::$number);
} catch (Error $error) {
    echo get_class($error), ': ', $error->getMessage(), "\n";
}
var_dump(isset(TypedStaticReference::$number));
var_dump(empty(TypedStaticReference::$number));

read_ref(TypedStaticReference::$number);
write_ref(TypedStaticReference::$number, '12');
var_dump(TypedStaticReference::$number);
try {
    write_ref(TypedStaticReference::$number, 'bad');
} catch (TypeError $error) {
    echo get_class($error), ': ', $error->getMessage(), "\n";
}
var_dump(TypedStaticReference::$number);

append_ref(TypedStaticReference::$items);
var_dump(TypedStaticReference::$items);
write_ref(TypedStaticReference::$number, null);
try {
    append_ref(TypedStaticReference::$number);
} catch (TypeError $error) {
    echo get_class($error), ': ', $error->getMessage(), "\n";
}
var_dump(TypedStaticReference::$number);

class UnionTypedStaticReference {
    public static int|string|null $value;
}

class IntTypedStaticReference {
    public static ?int $value;
}

class InitializedIntTypedStaticReference {
    public static int $value = 12;
}

class InitializedStringTypedStaticReference {
    public static string $value = '';
}

function exercise_shared_typed_static_references(): void {
    $shared =& UnionTypedStaticReference::$value;
    IntTypedStaticReference::$value =& $shared;
    try {
        write_ref($shared, '34');
    } catch (TypeError $error) {
        echo get_class($error), ': ', $error->getMessage(), "\n";
    }
    var_dump(UnionTypedStaticReference::$value, IntTypedStaticReference::$value);
    write_ref($shared, true);
    var_dump(UnionTypedStaticReference::$value, IntTypedStaticReference::$value);

    $initialized =& InitializedIntTypedStaticReference::$value;
    try {
        InitializedStringTypedStaticReference::$value =& $initialized;
    } catch (TypeError $error) {
        echo get_class($error), ': ', $error->getMessage(), "\n";
    }
    var_dump(
        InitializedIntTypedStaticReference::$value,
        InitializedStringTypedStaticReference::$value
    );
}

exercise_shared_typed_static_references();
