<?php

interface Reader extends Countable, IteratorAggregate {
    public function read(): string;
}

trait Logging {
    public function log(): void {
        echo "log";
    }
}

class UsesTrait {
    use Logging;
}
