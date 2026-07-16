<?php
// oracle-probe: id=oracle-builtin-behavior-function-mb-ord-6653a2d08d area=builtin_behavior kind=function symbol=mb_ord source=ext/mbstring/mbstring.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-mb-ord-6653a2d08d failure_category=builtin_behavior requires_ref_extension=mbstring
try {
    $result = \mb_ord("");
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
