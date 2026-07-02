<?php
// runtime-semantics: category=types expect=pass
class Box {
    public string|null $name;
}

$box = new Box();
echo $box->name, "\n";
