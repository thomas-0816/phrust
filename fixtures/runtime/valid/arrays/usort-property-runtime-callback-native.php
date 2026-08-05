<?php
final class NativePropertySorter
{
    public array $output = [3, 1, 2];

    public function compare(int $left, int $right): int
    {
        return $left <=> $right;
    }

    public function run(): string
    {
        var_dump(usort($this->output, [$this, 'compare']));
        return $this->output[0] . ',' . $this->output[1] . ',' . $this->output[2];
    }
}

$sorter = new NativePropertySorter();
var_dump($sorter->run());
