--TEST--
Generated wp.core-language: destructor runs after script end for globals
--DESCRIPTION--
module: wp.core-language
generated timestamp: 20260629T000000Z
generator version: wp-wave3-core-language-v1
reason: request-end cleanup destroys global objects after script body
oracle: Reference PHP 8.5.7
--FILE--
<?php
class WpWave3GlobalDestructor {
    public function __destruct() {
        echo "destruct-global\n";
    }
}

$globalObject = new WpWave3GlobalDestructor();
echo "body\n";
?>
--EXPECT--
body
destruct-global
