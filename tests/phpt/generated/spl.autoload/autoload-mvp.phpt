--TEST--
SPL generated autoload MVP preserves callback order, class lookup, and unregister
--FILE--
<?php
function first_loader($class) {
    echo "first:$class\n";
}

spl_autoload_register('first_loader');
spl_autoload_register(function ($class) {
    echo "second:$class\n";
});

echo count(spl_autoload_functions()), "\n";
class_exists('MissingClass');
echo spl_autoload_unregister('first_loader') ? "removed\n" : "missing\n";
echo count(spl_autoload_functions()), "\n";
spl_autoload_call('OtherClass');
?>
--EXPECT--
2
first:MissingClass
second:MissingClass
removed
1
second:OtherClass
