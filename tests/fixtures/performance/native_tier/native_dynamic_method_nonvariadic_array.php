<?php

class NativeDynamicHookTarget
{
    public $callbacks = array();

    public function add($name, $callback, $priority, $acceptedArgs)
    {
        $this->callbacks[$priority][$name] = array(
            'function' => $callback,
            'accepted_args' => $acceptedArgs,
        );
    }
}

function invoke_native_dynamic_hook(object $target): void
{
    $target->add('first', 'strlen', 0, 1);
    $target->add('second', 'trim', 10, 1);
}

$target = new NativeDynamicHookTarget();
invoke_native_dynamic_hook($target);
echo count($target->callbacks), "\n";
