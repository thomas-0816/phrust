<?php
// runtime-semantics: category=clone_with expect=known_gap known_gap=E_PHP_RUNTIME_UNSUPPORTED_CLONE_WITH_PROPERTY_RULES
class CloneWithPrivateGap {
    private string $name = "old";
}

$original = new CloneWithPrivateGap();
$copy = clone($original, ["name" => "new"]);
