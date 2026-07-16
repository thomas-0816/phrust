<?php
// oracle-probe: id=oracle-builtin-contract-function-mysqli-real-escape-string-82172c2931 area=builtin_contract kind=function symbol=mysqli_real_escape_string source=ext/mysqli/mysqli.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-mysqli-real-escape-string-82172c2931 failure_category=builtin_contract requires_ref_extension=mysqli
try {
    $result = \mysqli_real_escape_string();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
