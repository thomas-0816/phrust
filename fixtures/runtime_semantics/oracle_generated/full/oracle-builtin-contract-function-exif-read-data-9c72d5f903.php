<?php
// oracle-probe: id=oracle-builtin-contract-function-exif-read-data-9c72d5f903 area=builtin_contract kind=function symbol=exif_read_data source=ext/exif/exif.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-exif-read-data-9c72d5f903 failure_category=builtin_contract requires_ref_extension=exif
$name = "exif_read_data";
echo function_exists($name) ? "available\n" : "missing\n";
