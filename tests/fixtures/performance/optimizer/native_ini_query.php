<?php

function native_ini_current(string $name): string|false
{
    return ini_get($name);
}

function native_ini_configured(string $name): string|false
{
    return get_cfg_var($name);
}

function native_include_path(): string|false
{
    return get_include_path();
}

ini_set('memory_limit', '64M');
echo 'current=', native_ini_current('memory_limit'), "\n";
echo native_ini_current('phrust.missing.option') === false ? "missing\n" : "bad\n";
echo native_ini_configured('memory_limit') === false ? "cfg-missing\n" : "cfg-present\n";

ini_set('include_path', 'native-a:native-b');
echo 'path=', native_include_path(), "\n";
