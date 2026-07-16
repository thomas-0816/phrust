<?php
// oracle-probe: id=oracle-builtin-contract-function-mysqli-get-client-stats-e7620baf5a area=builtin_contract kind=function symbol=mysqli_get_client_stats source=ext/mysqli/mysqli.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-mysqli-get-client-stats-e7620baf5a failure_category=builtin_contract requires_ref_extension=mysqli
try {
    $result = \mysqli_get_client_stats(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
