<?php
// oracle-probe: id=oracle-builtin-contract-function-base64-decode-3f84a3b35f area=builtin_contract kind=function symbol=base64_decode source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-base64-decode-3f84a3b35f failure_category=builtin_contract
try {
    $result = \base64_decode();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
