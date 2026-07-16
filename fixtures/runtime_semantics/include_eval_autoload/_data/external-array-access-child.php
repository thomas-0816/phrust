<?php

class ExternalArrayAccessItem
{
    public function __construct(public string $name)
    {
    }
}

class ExternalArrayAccessList implements ArrayAccess
{
    private array $items;

    public function __construct(array $items)
    {
        $this->items = $items;
    }

    public function offsetExists(mixed $offset): bool
    {
        return isset($this->items[$offset]);
    }

    public function offsetGet(mixed $offset): mixed
    {
        $item = $this->items[$offset];
        if (is_array($item)) {
            $item = new ExternalArrayAccessItem($item['name']);
            $this->items[$offset] = $item;
        }
        return $item;
    }

    public function offsetSet(mixed $offset, mixed $value): void
    {
        $this->items[$offset] = $value;
    }

    public function offsetUnset(mixed $offset): void
    {
        unset($this->items[$offset]);
    }
}

function external_array_access_list(): ExternalArrayAccessList
{
    return new ExternalArrayAccessList([['name' => 'external item']]);
}
