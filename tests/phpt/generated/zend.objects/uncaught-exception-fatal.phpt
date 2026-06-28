--TEST--
Generated zend.objects: uncaught exception renders as a PHP fatal error
--DESCRIPTION--
module: zend.objects
generated timestamp: 20260627T000000Z
generator version: phpt-objects-classes-v1
reason: a top-level uncaught Throwable prints "Fatal error: Uncaught <Class>: <message>" with a #0 {main} stack trace, matching PHP
--FILE--
<?php
throw new RuntimeException("kaboom");
?>
--EXPECTF--
Fatal error: Uncaught RuntimeException: kaboom in %s:%d
Stack trace:
#0 {main}
  thrown in %s on line %d
