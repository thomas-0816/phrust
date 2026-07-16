<?php

final class ExternalBacktraceException extends InvalidArgumentException
{
    public static function create($position, $name)
    {
        $stack = debug_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS, 2);

        return new self(sprintf(
            '%s::%s(): Argument #%d (%s)',
            $stack[1]['class'],
            $stack[1]['function'],
            $position,
            $name
        ));
    }
}
