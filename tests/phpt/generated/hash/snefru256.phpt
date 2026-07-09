--TEST--
hash snefru256 alias and hmac vectors
--SKIPIF--
<?php if (!extension_loaded("hash")) die("skip hash extension not loaded"); ?>
--FILE--
<?php
foreach (["snefru", "snefru256"] as $algorithm) {
    var_dump(in_array($algorithm, hash_algos(), true));
    var_dump(in_array($algorithm, hash_hmac_algos(), true));
    echo $algorithm, " empty ", hash($algorithm, ""), "\n";
    echo $algorithm, " abc ", hash($algorithm, "abc"), "\n";
    echo $algorithm, " hmac ", hash_hmac($algorithm, "payload", "key"), "\n";

    $ctx = hash_init($algorithm);
    hash_update($ctx, "a");
    hash_update($ctx, "bc");
    echo $algorithm, " context ", hash_final($ctx), "\n";
}
?>
--EXPECT--
bool(true)
bool(true)
snefru empty 8617f366566a011837f4fb4ba5bedea2b892f3ed8b894023d16ae344b2be5881
snefru abc 7d033205647a2af3dc8339f6cb25643c33ebc622d32979c4b612b02c4903031b
snefru hmac 4069aae0bfcf515ae3dcf53c79ebf2f7742ea2298a4339c328634fc381c3914f
snefru context 7d033205647a2af3dc8339f6cb25643c33ebc622d32979c4b612b02c4903031b
bool(true)
bool(true)
snefru256 empty 8617f366566a011837f4fb4ba5bedea2b892f3ed8b894023d16ae344b2be5881
snefru256 abc 7d033205647a2af3dc8339f6cb25643c33ebc622d32979c4b612b02c4903031b
snefru256 hmac 4069aae0bfcf515ae3dcf53c79ebf2f7742ea2298a4339c328634fc381c3914f
snefru256 context 7d033205647a2af3dc8339f6cb25643c33ebc622d32979c4b612b02c4903031b
