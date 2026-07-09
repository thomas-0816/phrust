--TEST--
igbinary Redis and Memcached serializer option smoke
--SKIPIF--
<?php
if (!extension_loaded("igbinary")) die("skip igbinary extension not loaded");
if (!extension_loaded("redis")) die("skip redis extension not loaded");
if (!extension_loaded("memcached")) die("skip memcached extension not loaded");
?>
--FILE--
<?php
$payload = ["a" => 1, "b" => [false, null]];
$encoded = igbinary_serialize($payload);
var_dump(igbinary_unserialize($encoded));

$redis = new Redis();
var_dump($redis->setOption(Redis::OPT_SERIALIZER, Redis::SERIALIZER_IGBINARY));
var_dump($redis->getOption(Redis::OPT_SERIALIZER));
var_dump($redis->set("payload", $payload));
var_dump($redis->get("payload"));

$memcached = new Memcached();
var_dump($memcached->setOption(Memcached::OPT_SERIALIZER, Memcached::SERIALIZER_IGBINARY));
var_dump($memcached->getOption(Memcached::OPT_SERIALIZER));
var_dump($memcached->set("payload", $payload));
var_dump($memcached->get("payload"));
?>
--EXPECT--
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
int(2)
bool(true)
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
int(2)
bool(true)
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
