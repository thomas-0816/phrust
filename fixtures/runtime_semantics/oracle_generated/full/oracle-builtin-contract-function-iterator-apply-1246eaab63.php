<?php
// oracle-probe: id=oracle-builtin-contract-function-iterator-apply-1246eaab63 area=builtin_contract kind=function symbol=iterator_apply source=ext/spl/php_spl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-iterator-apply-1246eaab63 failure_category=builtin_contract requires_ref_extension=spl
try {
    $result = \iterator_apply();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
