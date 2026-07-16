<?php

class CrossUnitDefaultParent
{
    private $queued = array();
    protected $labels = array('ready' => 'ok');

    public function hasQueued($key)
    {
        return array_key_exists($key, $this->queued);
    }

    public function label($key)
    {
        return $this->labels[$key];
    }
}
