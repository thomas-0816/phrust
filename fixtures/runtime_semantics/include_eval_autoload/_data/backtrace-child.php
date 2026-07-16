<?php

final class ExternalBacktraceFixture
{
    public static function inner($hidden)
    {
        foreach (debug_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS, 2) as $index => $frame) {
            echo $index,
                ':', $frame['class'] ?? '-',
                ':', $frame['type'] ?? '-',
                ':', $frame['function'],
                ':', array_key_exists('args', $frame) ? 'args' : 'no-args',
                "\n";
        }
    }

    public static function outer()
    {
        self::inner('not exposed');
    }

    public static function makeException()
    {
        return ExternalBacktraceException::create(1, 'value');
    }
}
