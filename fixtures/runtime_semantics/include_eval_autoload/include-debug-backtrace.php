<?php

require __DIR__ . '/_data/backtrace-child.php';
require __DIR__ . '/_data/backtrace-exception.php';

ExternalBacktraceFixture::outer();

echo ExternalBacktraceFixture::makeException()->getMessage(), "\n";
