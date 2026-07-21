<?php
// runtime-fixture: kind=valid

class VariadicMethodArrayFixture {
    public function countArguments(...$arguments) {
        return count($arguments);
    }

    public function countTrailingArguments($fixed, ...$arguments) {
        return count($arguments);
    }
}

$fixture = new VariadicMethodArrayFixture();
echo $fixture->countArguments('value'), "\n";
echo $fixture->countTrailingArguments('fixed', 'value'), "\n";
