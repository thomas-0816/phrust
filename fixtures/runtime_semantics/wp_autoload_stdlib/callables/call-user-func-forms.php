<?php
// runtime-semantics: category=wp_autoload_stdlib expect=pass
function pack_b_suffix($value) {
    return $value . "-fn";
}

class PackBCallable {
    public static function stat($value) {
        return $value . "-static";
    }

    public function inst($value) {
        return $value . "-inst";
    }

    public function __invoke($value) {
        return $value . "-invoke";
    }
}

$object = new PackBCallable();
$closure = function ($value) {
    return $value . "-closure";
};

echo call_user_func("pack_b_suffix", "a"), "\n";
echo call_user_func([PackBCallable::class, "stat"], "b"), "\n";
echo call_user_func([$object, "inst"], "c"), "\n";
echo call_user_func($closure, "d"), "\n";
echo call_user_func($object, "e"), "\n";
echo call_user_func_array("pack_b_suffix", ["f"]), "\n";
