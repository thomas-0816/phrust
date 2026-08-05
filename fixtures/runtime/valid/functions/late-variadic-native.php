<?php
require __DIR__ . '/late-variadic-caller.inc';
require __DIR__ . '/late-variadic-target.inc';
var_dump(invoke_late_variadic_native());
var_dump(invoke_late_variadic_empty_native());
