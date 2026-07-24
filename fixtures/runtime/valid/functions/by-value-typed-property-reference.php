<?php

function typed_property_reference_size(array $value): int
{
    return count($value);
}

function typed_property_reference_remove_first(array $value): void
{
    unset($value[0]);
}

class TypedPropertyReferenceCaller
{
    public $values = [1, 2, 3, 4];

    private function typed(array $value): int
    {
        return count($value);
    }

    private function removeFirst(array $value): void
    {
        unset($value[0]);
    }

    public function run(): int
    {
        $alias = &$this->values;
        return $this->typed($alias);
    }

    public function runFunction(): int
    {
        $alias = &$this->values;
        return typed_property_reference_size($alias);
    }

    public function preservesCallerCow(): bool
    {
        $alias = &$this->values;
        $this->removeFirst($alias);
        return isset($this->values[0]);
    }

    public function preservesCallerCowThroughFunction(): bool
    {
        $alias = &$this->values;
        typed_property_reference_remove_first($alias);
        return isset($this->values[0]);
    }
}

$caller = new TypedPropertyReferenceCaller();
echo $caller->runFunction(), '|', $caller->run(), '|', count($caller->values), '|';
echo $caller->preservesCallerCow() ? 'kept' : 'removed', '|';
echo $caller->preservesCallerCowThroughFunction() ? 'kept' : 'removed';
echo "\n";
