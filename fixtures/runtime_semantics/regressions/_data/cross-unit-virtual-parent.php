<?php

class CrossUnitVirtualParent {
    public function run(): string {
        return $this->item();
    }

    public function item(): string {
        return 'parent';
    }
}
