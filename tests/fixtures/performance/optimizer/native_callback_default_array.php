<?php

function native_callback_default_target(int $value = 7, string $label = 'native'): string
{
    return $value . ':' . $label;
}

function native_callback_default_invoke(): string
{
    return call_user_func_array('native_callback_default_target', []);
}

for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = native_callback_default_invoke();
}

echo $result, "\n";
