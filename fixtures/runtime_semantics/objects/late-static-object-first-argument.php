<?php
// runtime-semantics: category=objects expect=pass

class LateStaticPayload
{
}

class LateStaticObjectArgument
{
    private const TOKEN = 'owner-ok';

    private static function read(object $payload): string
    {
        if (!is_object($payload)) {
            return 'invalid';
        }
        return static::TOKEN;
    }

    public static function run(): string
    {
        return static::read(new LateStaticPayload());
    }
}

echo LateStaticObjectArgument::run(), "\n";
