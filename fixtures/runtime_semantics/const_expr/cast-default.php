<?php
// runtime-semantics: category=const_expr expect=pass
function cast_default_fixture($value = (int) "42"): void
{
    echo $value, "\n";
}
cast_default_fixture();
