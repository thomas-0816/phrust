--TEST--
Generated wp.core-language: destructor runs when local object leaves scope
--DESCRIPTION--
module: wp.core-language
generated timestamp: 20260629T000000Z
generator version: wp-wave3-core-language-v1
reason: request cleanup relies on deterministic object destruction
oracle: Reference PHP 8.5.7
--FILE--
<?php
class WpWave3Destructor {
    private $name;

    public function __construct($name) {
        $this->name = $name;
    }

    public function __destruct() {
        echo "destruct:" . $this->name . "\n";
    }
}

function wp_wave3_destructor_scope() {
    $value = new WpWave3Destructor("local");
    echo "body\n";
}

wp_wave3_destructor_scope();
echo "after\n";
?>
--EXPECT--
body
destruct:local
after
