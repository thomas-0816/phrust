<?php
abstract class DynamicConstructorParent
{
    private string $value;

    final private function __construct(string $value)
    {
        $this->value = $value;
    }

    public static function make(string $class, string $value): self
    {
        return new $class($value);
    }

    public function value(): string
    {
        return $this->value;
    }
}
