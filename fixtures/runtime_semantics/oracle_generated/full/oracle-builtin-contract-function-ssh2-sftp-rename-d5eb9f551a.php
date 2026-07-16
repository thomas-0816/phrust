<?php
// oracle-probe: id=oracle-builtin-contract-function-ssh2-sftp-rename-d5eb9f551a area=builtin_contract kind=function symbol=ssh2_sftp_rename source=Rust extension registry expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-ssh2-sftp-rename-d5eb9f551a failure_category=builtin_contract requires_ref_extension=ssh2
try {
    $result = \ssh2_sftp_rename(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
