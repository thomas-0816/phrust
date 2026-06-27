<?php
// PHP accepts surplus positional arguments; they are ignored for binding but
// remain visible to func_get_args().
function one($a)
{
    return $a . '|' . implode(',', func_get_args());
}

echo one(1, 2);
