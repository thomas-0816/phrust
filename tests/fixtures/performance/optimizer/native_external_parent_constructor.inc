<?php

class NativeExternalParentConstructorBase
{
    public $count = 1;

    public function __construct(int $count = 4)
    {
        $this->count = $count;
    }
}
