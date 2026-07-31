<?php

final class PerfLinkedNativeMethods
{
    private int $slot = 20;

    public function __construct(private int $base)
    {
    }

    public function add(int $value): int
    {
        return $this->base + $value;
    }

    public static function twice(int $value): int
    {
        return $value * 2;
    }

    public static function increment(&$value): int
    {
        ++$value;
        return $value;
    }

    public function incrementInstance(&$value): int
    {
        ++$value;
        return $value;
    }

    public function &slot()
    {
        return $this->slot;
    }

    public function readSlot(): int
    {
        return $this->slot;
    }
}

final class PerfLinkedPlainMethods
{
    public function value(): int
    {
        return 5;
    }
}

class PerfLinkedOverrideBase
{
    public function value(): int
    {
        return 1;
    }

    protected function hidden(): int
    {
        return 9;
    }

    public function expose(): int
    {
        return $this->hidden();
    }
}

final class PerfLinkedOverrideChild extends PerfLinkedOverrideBase
{
    public function value(): int
    {
        return 2;
    }
}

final class PerfLinkedMagicMethods
{
    public function __call(string $name, array $arguments): string
    {
        return $name . ':' . $arguments[0];
    }
}
