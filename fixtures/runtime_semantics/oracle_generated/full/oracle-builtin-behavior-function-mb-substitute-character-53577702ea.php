<?php
// oracle-probe: id=oracle-builtin-behavior-function-mb-substitute-character-53577702ea area=builtin_behavior kind=function symbol=mb_substitute_character source=ext/mbstring/mbstring.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-mb-substitute-character-53577702ea failure_category=builtin_behavior requires_ref_extension=mbstring
try {
    $result = \mb_substitute_character("");
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
