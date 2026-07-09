--TEST--
redis deterministic fake backend options, scan, and transaction mode
--SKIPIF--
<?php if (!extension_loaded("redis")) die("skip redis extension not loaded"); ?>
--FILE--
<?php
$redis = new Redis();
var_dump(method_exists($redis, "getMode"));
var_dump(Redis::OPT_SERIALIZER);
var_dump(Redis::SERIALIZER_PHP);
var_dump(Redis::OPT_COMPRESSION);
var_dump(Redis::COMPRESSION_NONE);
var_dump(Redis::OPT_SCAN);
var_dump(Redis::SCAN_RETRY);
var_dump(Redis::ATOMIC);
var_dump(Redis::MULTI);
var_dump(Redis::PIPELINE);
var_dump($redis->pconnect("127.0.0.1"));
var_dump($redis->isConnected());
var_dump($redis->ping());
var_dump($redis->setOption(Redis::OPT_SERIALIZER, Redis::SERIALIZER_PHP));
var_dump($redis->getOption(Redis::OPT_SERIALIZER));
var_dump($redis->setOption(Redis::OPT_COMPRESSION, Redis::COMPRESSION_NONE));
var_dump($redis->getOption(Redis::OPT_COMPRESSION));
var_dump($redis->setex("ttl", 30, "value"));
var_dump($redis->expire("ttl", 60));
var_dump($redis->persist("ttl"));
var_dump($redis->ttl("ttl"));
$redis->set("alpha", "one");
$redis->set("beta", "two");
$iterator = null;
$scan = $redis->scan($iterator);
sort($scan);
var_dump($scan);
var_dump($redis->getMode());
var_dump($redis->multi() instanceof Redis);
var_dump($redis->getMode());
var_dump($redis->exec());
var_dump($redis->getMode());
var_dump($redis->pipeline() instanceof Redis);
var_dump($redis->getMode());
var_dump($redis->discard());
var_dump($redis->getMode());
var_dump($redis->close());
var_dump($redis->isConnected());
?>
--EXPECT--
bool(true)
int(1)
int(1)
int(7)
int(0)
int(4)
int(1)
int(0)
int(1)
int(2)
bool(true)
bool(true)
string(5) "+PONG"
bool(true)
int(1)
bool(true)
int(0)
bool(true)
bool(true)
bool(true)
int(-1)
array(3) {
  [0]=>
  string(5) "alpha"
  [1]=>
  string(4) "beta"
  [2]=>
  string(3) "ttl"
}
int(0)
bool(true)
int(1)
array(0) {
}
int(0)
bool(true)
int(2)
bool(true)
int(0)
bool(true)
bool(false)
