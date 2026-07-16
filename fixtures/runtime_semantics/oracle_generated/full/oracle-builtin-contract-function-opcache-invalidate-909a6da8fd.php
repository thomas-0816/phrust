<?php
// oracle-probe: id=oracle-builtin-contract-function-opcache-invalidate-909a6da8fd area=builtin_contract kind=function symbol=opcache_invalidate source=ext/opcache/opcache.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-opcache-invalidate-909a6da8fd failure_category=builtin_contract requires_ref_extension=opcache
try {
    $result = \opcache_invalidate();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
