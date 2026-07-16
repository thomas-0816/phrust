<?php

class LateExternalByRefPropertyCaller
{
    public array $items = array('original');

    public function run(): void
    {
        $this->callTarget();
    }

    private function callTarget(): void
    {
        late_external_by_ref_property_target($this->items);
        echo implode('|', $this->items), "\n";
    }
}
