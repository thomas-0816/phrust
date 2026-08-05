<?php

class NativePropertySort
{
    private $callbacks = array();

    public function add($priority, $value)
    {
        $this->callbacks[$priority] = $value;
        if (count($this->callbacks) > 1) {
            ksort($this->callbacks, SORT_NUMERIC);
        }
    }

    public function keys()
    {
        return array_keys($this->callbacks);
    }
}

$sort = new NativePropertySort();
$sort->add(20, 'later');
$sort->add(10, 'earlier');
$separator = '';
foreach ($sort->keys() as $key) {
    echo $separator, $key;
    $separator = ',';
}
echo "\n";
