<?php
// oracle-probe: id=oracle-builtin-contract-function-rawurldecode-4656f9ec93 area=builtin_contract kind=function symbol=rawurldecode source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-rawurldecode-4656f9ec93 failure_category=builtin_contract
try {
    $result = \rawurldecode();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
