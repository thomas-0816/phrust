<?php
// oracle-probe: id=oracle-builtin-contract-function-timezone-identifiers-list-31565d6114 area=builtin_contract kind=function symbol=timezone_identifiers_list source=ext/date/php_date.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-timezone-identifiers-list-31565d6114 failure_category=builtin_contract requires_ref_extension=date
try {
    $result = \timezone_identifiers_list(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
