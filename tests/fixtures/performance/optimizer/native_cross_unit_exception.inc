<?php

function native_external_exception(int $value): string
{
    try {
        if ($value > 0) {
            throw new RuntimeException("native-$value");
        }
        return 'no-throw';
    } catch (RuntimeException $exception) {
        return 'caught:' . $exception->getMessage();
    }
}
