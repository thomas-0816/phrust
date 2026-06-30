<?php
// runtime-semantics: category=clone_with expect=known_gap known_gap=E_PHP_VM_UNSUPPORTED_PROPERTY_MODIFIER
class CloneWithStaticGap {
    public static string $name = "old";
}

$original = new CloneWithStaticGap();
$copy = clone($original, ["name" => "new"]);
