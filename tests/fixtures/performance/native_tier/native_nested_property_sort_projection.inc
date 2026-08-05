<?php

class NativeNestedPropertySort
{
    private $callbacks = array();
    private $priorities = array();

    public function add($priority, $id, $value)
    {
        $priorityExisted = isset($this->callbacks[$priority]);
        $this->callbacks[$priority][$id] = array('value' => $value);
        if (!$priorityExisted && count($this->callbacks) > 1) {
            ksort($this->callbacks, SORT_NUMERIC);
        }
        $this->priorities = array_keys($this->callbacks);
    }

    public function printPriorities()
    {
        foreach ($this->priorities as $priority) {
            echo $priority, "\n";
        }
    }
}
