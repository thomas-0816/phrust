<?php
// runtime-semantics: category=objects expect=pass

class LateStaticPrivateSameClass
{
    private static function value(): string
    {
        return 'private-ok';
    }

    public static function read(): string
    {
        return static::value();
    }
}

echo LateStaticPrivateSameClass::read(), "\n";
