<?php
// oracle-probe: id=oracle-builtin-contract-function-ssh2-sftp-chmod-a7f66b95ad area=builtin_contract kind=function symbol=ssh2_sftp_chmod source=Rust extension registry expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-ssh2-sftp-chmod-a7f66b95ad failure_category=builtin_contract requires_ref_extension=ssh2
$name = "ssh2_sftp_chmod";
echo function_exists($name) ? "available\n" : "missing\n";
