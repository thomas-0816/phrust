<?php
// oracle-probe: id=oracle-builtin-contract-function-apcu-cache-info-52d9b22a53 area=builtin_contract kind=function symbol=apcu_cache_info source=Rust extension registry expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-apcu-cache-info-52d9b22a53 failure_category=builtin_contract requires_ref_extension=apcu
try {
    $result = \apcu_cache_info(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
