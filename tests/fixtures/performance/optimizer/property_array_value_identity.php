<?php

require_once __DIR__ . '/property_array_value_identity.inc';

class PropertyArrayValueIdentityLocale {
    public $weekday = [];
    public $weekday_initial = [];

    public function init(): void {
        $unrelated = new PropertyArrayValueIdentityMarker();
        $translated = property_array_value_identity_translate("Sunday");
        $this->weekday[0] = $translated;
        $key = $this->weekday[0];
        $this->weekday_initial[$key] = "S";
        var_dump($key);
        var_dump($this->weekday_initial);
        var_dump($unrelated);
    }
}

$locale = new PropertyArrayValueIdentityLocale();
$locale->init();
