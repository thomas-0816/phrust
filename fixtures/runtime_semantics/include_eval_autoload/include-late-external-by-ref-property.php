<?php
// runtime-semantics: expect=pass

include __DIR__ . '/_data/late-external-by-ref-property-caller.php';
include __DIR__ . '/_data/late-external-by-ref-property-target.php';

$caller = new LateExternalByRefPropertyCaller();
$caller->run();
