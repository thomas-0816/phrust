<?php
class DynamicPropertyDimensionProbe {
    public array $items = [
        'present' => [7],
        'null' => [null],
        'empty' => [''],
    ];

    public function probe(string $property): void {
        var_dump(isset($this->{$property}['present'][0]));
        var_dump(isset($this->{$property}['null'][0]));
        var_dump(isset($this->{$property}['missing'][0]));
        var_dump(empty($this->{$property}['present'][0]));
        var_dump(empty($this->{$property}['empty'][0]));
        var_dump(empty($this->{$property}['missing'][0]));
    }
}

(new DynamicPropertyDimensionProbe())->probe('items');
