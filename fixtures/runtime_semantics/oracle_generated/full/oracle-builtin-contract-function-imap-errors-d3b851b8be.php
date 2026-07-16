<?php
// oracle-probe: id=oracle-builtin-contract-function-imap-errors-d3b851b8be area=builtin_contract kind=function symbol=imap_errors source=Rust extension registry expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-imap-errors-d3b851b8be failure_category=builtin_contract requires_ref_extension=imap
try {
    $result = \imap_errors(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
