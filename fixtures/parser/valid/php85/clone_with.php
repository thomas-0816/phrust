<?php

class CloneSubject {
    public function __construct(public string $foo = "foo") {}
}

$subject = new CloneSubject();
$copy = clone($subject, ["foo" => "updated"]);
$plain = clone $subject;
