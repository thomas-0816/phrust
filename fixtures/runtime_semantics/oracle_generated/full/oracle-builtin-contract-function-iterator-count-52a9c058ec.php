<?php
// oracle-probe: id=oracle-builtin-contract-function-iterator-count-52a9c058ec area=builtin_contract kind=function symbol=iterator_count source=ext/spl/php_spl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-iterator-count-52a9c058ec failure_category=builtin_contract requires_ref_extension=spl
$name = "iterator_count";
echo function_exists($name) ? "available\n" : "missing\n";
