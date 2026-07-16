<?php
// oracle-probe: id=oracle-builtin-behavior-function-mb-lcfirst-0f1b5b1f97 area=builtin_behavior kind=function symbol=mb_lcfirst source=ext/mbstring/mbstring.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-mb-lcfirst-0f1b5b1f97 failure_category=builtin_behavior requires_ref_extension=mbstring
try {
    $result = \mb_lcfirst([]);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
