<?php

function weak_backtrace_arguments(): void
{
    $trace = debug_backtrace(false, '1');
    echo is_array($trace) ? 'array' : 'other';
    echo "\n";
}

weak_backtrace_arguments();
