<?php
// runtime-semantics: category=types expect=pass
// PHP reference: readonly properties can be initialized once and then read normally.
class ReadonlyFixtureBox
{
    public readonly int $value;

    public function __construct(int $value)
    {
        $this->value = $value;
    }
}

$box = new ReadonlyFixtureBox(9);
echo $box->value, "\n";
