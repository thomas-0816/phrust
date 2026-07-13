--TEST--
msgpack constants plus Redis and Memcached serializer option smoke
--SKIPIF--
<?php
if (!extension_loaded("msgpack")) die("skip msgpack extension not loaded");
if (!extension_loaded("redis")) die("skip redis extension not loaded");
if (!extension_loaded("memcached")) die("skip memcached extension not loaded");
?>
--FILE--
<?php
$payload = ["a" => 1, "b" => [false, null]];
$encoded = msgpack_pack($payload);
var_dump(MESSAGEPACK_OPT_PHPONLY);
var_dump(MESSAGEPACK_OPT_ASSOC);
var_dump(MESSAGEPACK_OPT_FORCE_F32);
var_dump(msgpack_unpack($encoded));

$redis = new Redis();
var_dump($redis->setOption(Redis::OPT_SERIALIZER, Redis::SERIALIZER_MSGPACK));
var_dump($redis->getOption(Redis::OPT_SERIALIZER));

$memcached = new Memcached();
var_dump(Memcached::SERIALIZER_MSGPACK);
var_dump($memcached->setOption(Memcached::OPT_SERIALIZER, Memcached::SERIALIZER_MSGPACK));
var_dump($memcached->getOption(Memcached::OPT_SERIALIZER));
?>
--EXPECT--
int(-1001)
int(-1002)
int(-1003)
array(2) {
  ["a"]=>
  int(1)
  ["b"]=>
  array(2) {
    [0]=>
    bool(false)
    [1]=>
    NULL
  }
}
bool(true)
int(3)
int(5)
bool(true)
int(5)
