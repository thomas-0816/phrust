<?php

interface NamedInterface {}

class BaseClass {}

final class User extends BaseClass implements NamedInterface {
    public const VERSION = 1;
    public string $name;

    public function __construct(string $name) {
        echo $name;
    }
}

$anonymous = new class("name") extends BaseClass implements NamedInterface {
    public string $name;

    public function __construct(string $name) {
        echo $name;
    }
};
