<?php
// oracle-probe: id=oracle-builtin-behavior-function-mhash-0d16212d84 area=builtin_behavior kind=function symbol=mhash source=ext/hash/hash.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-mhash-0d16212d84 failure_category=builtin_behavior requires_ref_extension=hash
try {
    $result = \mhash(algo: 0, data: "");
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
