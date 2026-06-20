<?php

trait PrimaryTrait {
    public function run(): void {}
}

trait SecondaryTrait {
    public function run(): void {}
}

abstract class MemberExamples {
    public const string KIND = "member", OTHER = "other";
    private static int $count = 0;
    public private(set) string $name;
    protected array $items = [], $moreItems = [];

    use PrimaryTrait, SecondaryTrait {
        PrimaryTrait::run insteadof SecondaryTrait;
        SecondaryTrait::run as secondaryRun;
    }

    final public function method(string $value): void {
        echo $value;
    }

    abstract protected function deferred(): void;
}
