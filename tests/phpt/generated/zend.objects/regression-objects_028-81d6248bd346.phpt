--TEST--
PHPT generated regression: Testing 'static::' and 'parent::' in calls
--DESCRIPTION--
original php-src path: Zend/tests/objects/objects_028.phpt
original source hash: 81d6248bd3469c9015be9f71a48c4a5a2d7dbbf02755815b4b2f5efc9b76caae
generated timestamp: 20260625T154035Z
generator version: phpt-generate-v1
reason: known target failure minimized against reference output
--FILE--
<?php
class bar {
    public function __call($a, $b) {
        print "hello\n";
    }
}
class foo extends bar {
    public function __construct() {
        static::bar();
        parent::bar();
    }
}
new foo;
--EXPECT--
hello
hello
