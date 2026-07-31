<?php

namespace Fixture\External;

class ExternalNestedListener
{
    protected $state = array();

    public function register(ExternalNestedHookManager $hooks)
    {
        $hooks->add('event', 0, array($this, 'early'));
        $hooks->add('other', 0, array($this, 'late'));
    }

    public function late($value)
    {
        echo 'late-', $value, "\n";
    }

    public function early($value)
    {
        echo 'early-', $value, "\n";
    }
}

function make_external_nested_listener()
{
    return new ExternalNestedListener();
}
