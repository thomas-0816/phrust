<?php
eval('class EvalDeclaredClassFixture { public function value() { return 1; } }');
$object = new EvalDeclaredClassFixture();
echo $object->value(), "\n";
