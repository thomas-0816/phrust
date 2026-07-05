<?php
// runtime-semantics: category=known_gaps expect=known_gap known_gap=E_PHP_VM_BY_REF_OVERLOADED_DIM_GAP
// PHP reference: passing an overloaded (ArrayAccess) element to a by-ref
// parameter calls offsetGet, emits the indirect-modification notice, and
// continues with no effect on the container.
class OverloadedDimBox implements ArrayAccess
{
    private array $data = ['k' => ['v']];

    public function offsetExists(mixed $offset): bool
    {
        return isset($this->data[$offset]);
    }

    public function offsetGet(mixed $offset): mixed
    {
        echo "get\n";
        return $this->data[$offset];
    }

    public function offsetSet(mixed $offset, mixed $value): void
    {
        $this->data[$offset] = $value;
    }

    public function offsetUnset(mixed $offset): void
    {
        unset($this->data[$offset]);
    }
}

function take(array &$x): void
{
    $x[] = 'w';
}

$box = new OverloadedDimBox();
take($box['k']);
var_dump($box['k']);
