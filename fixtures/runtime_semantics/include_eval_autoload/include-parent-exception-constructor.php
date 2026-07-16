<?php

require __DIR__ . '/_data/parent-exception-child.php';

$exception = new Fixture\Throwable\ExternalException('failure', 'transport', 17);
try {
    throw $exception;
} catch (Fixture\Throwable\ExternalException $caught) {
    var_dump($caught->getMessage(), $caught->getCode(), $caught->state());
}
