<?php

class ExternalIteratorCookie
{
    public function __construct(public string $name)
    {
    }
}

class ExternalIteratorJar implements IteratorAggregate
{
    protected array $cookies;

    public function __construct()
    {
        $this->cookies = [
            new ExternalIteratorCookie('first'),
            new ExternalIteratorCookie('second'),
        ];
    }

    public function getIterator(): Traversable
    {
        return new ArrayIterator($this->cookies);
    }
}

function external_iterator_jar(): ExternalIteratorJar
{
    return new ExternalIteratorJar();
}

class ExternalDirectIterator implements Iterator
{
    private array $items = [
        ['name' => 'direct first'],
        ['name' => 'direct second'],
    ];
    private int $position = 0;

    public function rewind(): void
    {
        $this->position = 0;
    }

    public function valid(): bool
    {
        return isset($this->items[$this->position]);
    }

    public function current(): mixed
    {
        $item = $this->items[$this->position];
        if (is_array($item)) {
            $item = new ExternalIteratorCookie($item['name']);
            $this->items[$this->position] = $item;
        }
        return $item;
    }

    public function key(): mixed
    {
        return $this->position;
    }

    public function next(): void
    {
        ++$this->position;
    }
}

function external_direct_iterator(): ExternalDirectIterator
{
    return new ExternalDirectIterator();
}
