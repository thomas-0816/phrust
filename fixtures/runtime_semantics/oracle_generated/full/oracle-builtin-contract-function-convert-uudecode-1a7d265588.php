<?php
// oracle-probe: id=oracle-builtin-contract-function-convert-uudecode-1a7d265588 area=builtin_contract kind=function symbol=convert_uudecode source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-convert-uudecode-1a7d265588 failure_category=builtin_contract
try {
    $result = \convert_uudecode();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
