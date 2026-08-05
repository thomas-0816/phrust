<?php

class NativePropertySortForeach
{
    private $callbacks = array();

    public function add($priority, $value)
    {
        $this->callbacks[$priority] = $value;
        if (count($this->callbacks) > 1) {
            ksort($this->callbacks, SORT_NUMERIC);
        }
    }

    public function printKeys()
    {
        foreach ($this->callbacks as $priority => $value) {
            echo $priority, ':', $value, "\n";
        }
    }
}

$sort = new NativePropertySortForeach();
$sort->add(20, 'later');
$sort->add(10, 'earlier');
$sort->printKeys();
