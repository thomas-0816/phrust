<?php
// runtime-semantics: category=include_eval_autoload expect=pass php_ref_required=1
// Output emitted by an overridden method in another compiled unit must flow
// through its inherited caller and into the caller's active output buffer.

require __DIR__ . '/_data/cross-unit-output-base.php';
require __DIR__ . '/_data/cross-unit-output-child.php';

$emitter = new CrossUnitOutputChild();
ob_start();
$count = $emitter->run(['one', 'two']);
$captured = ob_get_clean();
echo $count, ':', strlen($captured), ':', $captured, "\n";
