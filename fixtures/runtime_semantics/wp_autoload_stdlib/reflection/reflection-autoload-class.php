<?php
// runtime-semantics: category=wp_autoload_stdlib expect=pass
spl_autoload_register(function ($class) {
    if ($class === "PackBReflectionTarget") {
        eval('class PackBReflectionTarget { public const VALUE = "ref"; public function run() {} }');
    }
});

$reflection = new ReflectionClass("PackBReflectionTarget");
echo $reflection->getName(), "|";
echo $reflection->hasMethod("run") ? "method" : "missing";
echo "|", $reflection->getConstant("VALUE"), "\n";
