<?php

function native_extension_is_loaded(string $name): bool
{
    return extension_loaded($name);
}

function native_loaded_extensions(bool $zendOnly = false): array
{
    return get_loaded_extensions($zendOnly);
}

echo native_extension_is_loaded('json') ? "json\n" : "bad\n";
echo native_extension_is_loaded('JSON') ? "case\n" : "bad\n";
echo native_extension_is_loaded('phrust_missing_extension') ? "bad\n" : "missing\n";

$loaded = native_loaded_extensions();
echo in_array('json', $loaded, true) ? "listed\n" : "bad\n";
echo is_array(native_loaded_extensions(true)) ? "zend-array\n" : "bad\n";
