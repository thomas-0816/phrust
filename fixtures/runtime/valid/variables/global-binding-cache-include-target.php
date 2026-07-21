<?php

function bump_included_cached_global(): int
{
    global $included_counter;
    return ++$included_counter;
}
