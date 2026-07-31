<?php
// runtime-semantics: category=errors expect=pass
// Exact request-local error_get_last()/error_clear_last() state boundary.

$previousLevel = error_reporting(0);
error_clear_last();
trigger_error("native error-state marker", E_USER_NOTICE);

function nativeErrorStateFamily(): array
{
    $last = error_get_last();
    $cleared = error_clear_last();
    $empty = error_get_last();
    return [$last, $cleared, $empty];
}

var_dump(nativeErrorStateFamily());
error_reporting($previousLevel);
