<?php
// oracle-probe: id=oracle-builtin-contract-function-mysqli-query-067c16f983 area=builtin_contract kind=function symbol=mysqli_query source=ext/mysqli/mysqli.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-mysqli-query-067c16f983 failure_category=builtin_contract requires_ref_extension=mysqli
try {
    $result = \mysqli_query();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
