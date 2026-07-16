<?php
// oracle-probe: id=oracle-builtin-contract-function-mysqli-stmt-bind-result-d65129d2f2 area=builtin_contract kind=function symbol=mysqli_stmt_bind_result source=ext/mysqli/mysqli.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-mysqli-stmt-bind-result-d65129d2f2 failure_category=builtin_contract requires_ref_extension=mysqli
try {
    $result = \mysqli_stmt_bind_result();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
