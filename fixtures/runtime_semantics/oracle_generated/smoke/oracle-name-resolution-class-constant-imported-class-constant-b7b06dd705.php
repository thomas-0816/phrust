<?php
// oracle-probe: id=oracle-name-resolution-class-constant-imported-class-constant-b7b06dd705 area=name_resolution kind=class_constant symbol=imported-class-constant source=seed expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-name-resolution-class-constant-imported-class-constant-b7b06dd705 failure_category=name_resolution
namespace OracleProbe\Lib { class Box { public const VALUE = "ok"; } }
namespace OracleProbe\App { use OracleProbe\Lib\Box; echo Box::VALUE, "\n"; }
