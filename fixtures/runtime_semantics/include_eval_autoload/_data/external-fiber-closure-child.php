<?php

const EXTERNAL_FIBER_PREFIX = 'included';

function external_fiber_format(string $value): string
{
    return EXTERNAL_FIBER_PREFIX . ':' . $value;
}

final class ExternalFiberFactory
{
    private const SCOPE = 'private-scope';

    public static function make(): Closure
    {
        return function (string $value): string {
            $before = external_fiber_format($value) . ':' . self::SCOPE;
            $resumed = Fiber::suspend($before);
            return external_fiber_format($resumed) . ':' . self::SCOPE;
        };
    }
}

return ExternalFiberFactory::make();
