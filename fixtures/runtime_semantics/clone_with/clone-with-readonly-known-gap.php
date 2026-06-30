<?php
// runtime-semantics: category=clone_with expect=known_gap known_gap=E_PHP_RUNTIME_UNSUPPORTED_CLONE_WITH_PROPERTY_RULES
class CloneWithReadonlyGap {
    public readonly string $name;

    public function __construct() {
        $this->name = "old";
    }
}

$original = new CloneWithReadonlyGap();
$copy = clone($original, ["name" => "new"]);
