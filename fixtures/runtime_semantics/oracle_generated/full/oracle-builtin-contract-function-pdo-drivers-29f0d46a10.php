<?php
// oracle-probe: id=oracle-builtin-contract-function-pdo-drivers-29f0d46a10 area=builtin_contract kind=function symbol=pdo_drivers source=ext/pdo/pdo.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-pdo-drivers-29f0d46a10 failure_category=builtin_contract requires_ref_extension=pdo
try {
    $result = \pdo_drivers(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
