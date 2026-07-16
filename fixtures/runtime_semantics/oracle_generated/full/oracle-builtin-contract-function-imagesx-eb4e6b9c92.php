<?php
// oracle-probe: id=oracle-builtin-contract-function-imagesx-eb4e6b9c92 area=builtin_contract kind=function symbol=imagesx source=ext/gd/gd.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-imagesx-eb4e6b9c92 failure_category=builtin_contract requires_ref_extension=gd
try {
    $result = \imagesx();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
