--TEST--
redis endpoint-backed client fails closed without a configured daemon
--SKIPIF--
<?php if (!extension_loaded("redis")) die("skip redis extension not loaded"); ?>
--FILE--
<?php
var_dump(extension_loaded("redis"));
var_dump(class_exists("Redis", false));
$redis = new Redis();
var_dump($redis instanceof Redis);
var_dump(method_exists($redis, "getMultiple"));
var_dump($redis->connect("127.0.0.1", 1, 0.001));
var_dump($redis->isConnected());
var_dump($redis->ping());
var_dump($redis->set("alpha", "one"));
var_dump($redis->get("alpha"));
var_dump($redis->setex("alpha", 30, "one"));
var_dump($redis->setnx("alpha", "two"));
var_dump($redis->del("alpha", "missing"));
var_dump($redis->exists("alpha", "beta"));
var_dump($redis->incr("count"));
var_dump($redis->decr("count"));
var_dump($redis->mset(["beta" => "two", "gamma" => "three"]));
var_dump($redis->mget(["alpha", "beta", "missing"]));
var_dump($redis->hSet("hash", "field", "value"));
var_dump($redis->hGet("hash", "field"));
var_dump($redis->lPush("list", "left", "middle"));
var_dump($redis->rPush("list", "right"));
var_dump($redis->lRange("list", 0, -1));
var_dump($redis->expire("alpha", 60));
var_dump($redis->ttl("alpha"));
?>
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
