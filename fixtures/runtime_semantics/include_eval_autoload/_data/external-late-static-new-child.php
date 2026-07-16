<?php

class ExternalLateStaticBase
{
    public static function create(): static
    {
        return new static();
    }
}

class ExternalLateStaticChild extends ExternalLateStaticBase
{
}
