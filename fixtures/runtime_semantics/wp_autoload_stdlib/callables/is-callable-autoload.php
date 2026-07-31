<?php
// runtime-semantics: category=wp_autoload_stdlib expect=pass
$autoloaded = [];
spl_autoload_register(function ($class) {
    global $autoloaded;
    $autoloaded[] = $class;
    if ($class === "PackBCallableAutoload") {
        eval('class PackBCallableAutoload { public static function boot() { return "boot"; } }');
    }
});

$name = null;
var_dump(is_callable(["PackBCallableAutoload", "boot"], false, $name));
var_dump($name);
echo call_user_func(["PackBCallableAutoload", "boot"]), "\n";

$name = null;
var_dump(is_callable(["NeverLoadedBySyntaxCheck", "boot"], true, $name));
var_dump($name);
var_dump($autoloaded);

class CallableMagicSurface
{
    private function hidden() {}
    public function visible() {}
    public function __call($name, $arguments) {}
    public static function __callStatic($name, $arguments) {}
    public function __invoke() {}
}

$object = new CallableMagicSurface();
foreach ([
    [$object, "visible"],
    [$object, "hidden"],
    [$object, "missing"],
    ["CallableMagicSurface", "missing"],
    $object,
    42,
] as $candidate) {
    $name = null;
    var_dump(is_callable($candidate, false, $name));
    var_dump($name);
}
