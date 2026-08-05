<?php
function invoke_late_variadic_native(): string
{
    return late_variadic_native('head', 10, 20, 30);
}

function invoke_late_variadic_empty_native(): string
{
    return late_variadic_native('empty');
}
