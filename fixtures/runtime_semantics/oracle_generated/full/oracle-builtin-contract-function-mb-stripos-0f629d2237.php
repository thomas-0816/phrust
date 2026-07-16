<?php
// oracle-probe: id=oracle-builtin-contract-function-mb-stripos-0f629d2237 area=builtin_contract kind=function symbol=mb_stripos source=ext/mbstring/mbstring.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-mb-stripos-0f629d2237 failure_category=builtin_contract requires_ref_extension=mbstring
try {
    $result = \mb_stripos();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
