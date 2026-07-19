<?php

function missing_global_dimension_is_set(): bool
{
    if (!isset($GLOBALS['missing_global_dimension'])) {
        return false;
    }

    return $GLOBALS['missing_global_dimension']->in_admin();
}

echo missing_global_dimension_is_set() ? "set\n" : "missing\n";
