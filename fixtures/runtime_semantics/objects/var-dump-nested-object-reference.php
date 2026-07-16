<?php

class NestedDumpFixture
{
    public int $visible = 1;
    private int $hidden = 2;
}

$referenced = 7;
var_dump([new NestedDumpFixture(), &$referenced, [new NestedDumpFixture()]]);
