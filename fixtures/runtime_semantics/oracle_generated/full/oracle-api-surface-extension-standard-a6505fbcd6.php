<?php
// oracle-probe: id=oracle-api-surface-extension-standard-a6505fbcd6 area=api_surface kind=extension symbol=standard source=reference-php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-api-surface-extension-standard-a6505fbcd6 failure_category=api_surface
echo extension_loaded("standard") ? "extension\n" : "missing\n";
