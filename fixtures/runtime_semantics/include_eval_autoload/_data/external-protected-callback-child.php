<?php

final class ExternalProtectedCallback
{
    public static function run(string $value): string
    {
        return call_user_func(
            [self::class, 'decorate'],
            $value,
        );
    }

    protected static function decorate(string $value): string
    {
        return "protected {$value}";
    }
}
