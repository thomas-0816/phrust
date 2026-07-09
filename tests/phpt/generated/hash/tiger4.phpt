--TEST--
hash tiger 4-pass vectors
--SKIPIF--
<?php if (!extension_loaded("hash")) die("skip hash extension not loaded"); ?>
--FILE--
<?php
$inputs = [
    "",
    "abc",
    str_repeat("a", 63),
    str_repeat("abc", 61),
    str_repeat("abc", 64),
];

foreach (["tiger128,4", "tiger160,4", "tiger192,4"] as $algorithm) {
    var_dump(in_array($algorithm, hash_algos(), true));
    var_dump(in_array($algorithm, hash_hmac_algos(), true));
    foreach ($inputs as $input) {
        echo $algorithm, " ", strlen($input), " ", hash($algorithm, $input), "\n";
    }
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
tiger128,4 0 24cc78a7f6ff3546e7984e59695ca13d
tiger128,4 3 538883c8fc5f28250299018e66bdf4fd
tiger128,4 63 fe897ca63f7389d73c025b32f4bdce50
tiger128,4 183 db07f9f768a7e7bc04c55f5ddcb91125
tiger128,4 192 81011bea75be6133fc37c95050e3c7f0
tiger128,4 hmac 8a398c914ecc1837438befc56ce98f7c
tiger128,4 context 538883c8fc5f28250299018e66bdf4fd
bool(true)
bool(true)
tiger160,4 0 24cc78a7f6ff3546e7984e59695ca13d804e0b68
tiger160,4 3 538883c8fc5f28250299018e66bdf4fdb5ef7b65
tiger160,4 63 fe897ca63f7389d73c025b32f4bdce503a48d310
tiger160,4 183 db07f9f768a7e7bc04c55f5ddcb91125d66c0a4a
tiger160,4 192 81011bea75be6133fc37c95050e3c7f09c2b568c
tiger160,4 hmac 04939206562668d7e2ba5414d63327cd609c13ed
tiger160,4 context 538883c8fc5f28250299018e66bdf4fdb5ef7b65
bool(true)
bool(true)
tiger192,4 0 24cc78a7f6ff3546e7984e59695ca13d804e0b686e255194
tiger192,4 3 538883c8fc5f28250299018e66bdf4fdb5ef7b65f2e91753
tiger192,4 63 fe897ca63f7389d73c025b32f4bdce503a48d310a20f7211
tiger192,4 183 db07f9f768a7e7bc04c55f5ddcb91125d66c0a4a4d8dba68
tiger192,4 192 81011bea75be6133fc37c95050e3c7f09c2b568c7936fbb3
tiger192,4 hmac 5980cb8fc54fd79616ba13298e56846210013fb0d11a45e5
tiger192,4 context 538883c8fc5f28250299018e66bdf4fdb5ef7b65f2e91753
