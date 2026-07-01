<?php
// runtime-semantics: category=wp_autoload_stdlib expect=pass
spl_autoload_register(function ($class) {
    if ($class === "PackBCallableAutoload") {
        eval('class PackBCallableAutoload { public static function boot() { return "boot"; } }');
    }
});

var_dump(is_callable(["PackBCallableAutoload", "boot"]));
echo call_user_func(["PackBCallableAutoload", "boot"]), "\n";
