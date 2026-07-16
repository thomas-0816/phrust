<?php

final class LazyCrossUnitTaxonomy {
    protected static $default_labels = array();

    public $name;
    public $labels;

    public function __construct($name) {
        $this->name = $name;
        $this->labels = array();
        $this->labels = lazy_cross_unit_labels($this);
    }

    public static function get_default_labels() {
        if (!empty(self::$default_labels)) {
            return self::$default_labels;
        }

        self::$default_labels = array('name' => 'Categories');
        return self::$default_labels;
    }
}
