--TEST--
Generated wp.core-language: nullsafe property and method access
--DESCRIPTION--
module: wp.core-language
generated timestamp: 20260629T000000Z
generator version: wp-wave3-core-language-v1
reason: modern libraries use nullsafe chains for optional service objects
oracle: Reference PHP 8.5.7
--FILE--
<?php
class WpWave3NullsafeBox {
    public $name;
    public $next = null;

    public function __construct($name) {
        $this->name = $name;
    }

    public function child() {
        return $this->next;
    }

    public function label() {
        return "label:" . $this->name;
    }
}

$root = new WpWave3NullsafeBox("root");
$root->next = new WpWave3NullsafeBox("child");
$missing = null;
echo (($root?->child()?->label()) ?? "none"), "\n";
echo (($missing?->child()?->label()) ?? "none"), "\n";
?>
--EXPECT--
label:child
none
