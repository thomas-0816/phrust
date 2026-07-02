<?php
// oracle-probe: id=oracle-name-resolution-array-key-class-constant-key-df146209cb area=name_resolution kind=array-key symbol=class-constant-key source=seed expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-name-resolution-array-key-class-constant-key-df146209cb failure_category=name_resolution
class OracleKeyBox { public const KEY = "answer"; } $items = [OracleKeyBox::KEY => 42]; echo $items["answer"], "\n";
