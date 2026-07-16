<?php
// runtime-semantics: expect=pass regression_category=objects reference_behavior=stdout:false|ok regression_case=cross-unit-inheritance

require __DIR__ . '/_data/cross-unit-default-parent.php';
require __DIR__ . '/_data/cross-unit-default-child.php';

$child = new CrossUnitDefaultChild();
echo $child->hasQueued('missing') ? 'true' : 'false';
echo '|', $child->label('ready'), "\n";
