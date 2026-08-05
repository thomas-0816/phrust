<?php

#[AllowDynamicProperties]
class NativeComputedPropertyBase
{
}

class NativeComputedPropertyTarget extends NativeComputedPropertyBase
{
    public function publish($properties)
    {
        foreach ($properties as $name => $value) {
            $this->$name = $value;
        }
    }
}

$target = new NativeComputedPropertyTarget();
$target->publish(array('prefix' => 'wp_', 'siteid' => 7));
echo isset($target->prefix), isset($target->siteid), "\n";
