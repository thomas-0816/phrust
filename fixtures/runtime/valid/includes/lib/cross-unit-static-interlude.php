<?php
function cross_unit_static_interlude() {
    return 'interlude';
}

class CrossUnitStaticStorage {
    public function get($key) {
        return false;
    }
}

function cross_unit_interlude_storage() {
    static $storage = null;
    if (null === $storage) {
        $storage = new CrossUnitStaticStorage();
    }
    return $storage;
}

function cross_unit_make_nested() {
    return new CrossUnitNestedConstructor();
}
