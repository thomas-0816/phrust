--TEST--
redis opt-in live endpoint core commands
--SKIPIF--
<?php
if (!extension_loaded("redis")) die("skip redis extension not loaded");
if (!getenv("PHRUST_REDIS_LIVE_ENDPOINT")) die("skip PHRUST_REDIS_LIVE_ENDPOINT not set");
?>
--FILE--
<?php
$endpoint = getenv("PHRUST_REDIS_LIVE_ENDPOINT");
$parts = explode(":", $endpoint, 2);
$host = $parts[0];
$port = isset($parts[1]) ? (int) $parts[1] : 6379;
$redis = new Redis();
var_dump($redis->connect($host, $port, 1.0));
$prefix = "phrust:redis:live:";
$redis->del([
    $prefix . "alpha",
    $prefix . "beta",
    $prefix . "gamma",
    $prefix . "hash",
    $prefix . "list",
    $prefix . "count",
    $prefix . "ttl",
]);
var_dump($redis->ping());
var_dump($redis->set($prefix . "alpha", "one"));
var_dump($redis->get($prefix . "alpha"));
var_dump($redis->setex($prefix . "ttl", 30, "value"));
var_dump($redis->ttl($prefix . "ttl") >= 0);
var_dump($redis->mset([$prefix . "beta" => "two", $prefix . "gamma" => "three"]));
$many = $redis->mget([$prefix . "alpha", $prefix . "beta", $prefix . "missing"]);
var_dump($many[0], $many[1], $many[2]);
var_dump($redis->incr($prefix . "count"));
var_dump($redis->decr($prefix . "count"));
var_dump($redis->hSet($prefix . "hash", "field", "value"));
var_dump($redis->hGet($prefix . "hash", "field"));
var_dump($redis->lPush($prefix . "list", "left", "middle"));
var_dump($redis->rPush($prefix . "list", "right"));
var_dump($redis->lRange($prefix . "list", 0, -1));
$redis->del([
    $prefix . "alpha",
    $prefix . "beta",
    $prefix . "gamma",
    $prefix . "hash",
    $prefix . "list",
    $prefix . "count",
    $prefix . "ttl",
]);
?>
--EXPECT--
bool(true)
string(4) "PONG"
bool(true)
string(3) "one"
bool(true)
bool(true)
bool(true)
string(3) "one"
string(3) "two"
bool(false)
int(1)
int(0)
int(1)
string(5) "value"
int(2)
int(3)
array(3) {
  [0]=>
  string(6) "middle"
  [1]=>
  string(4) "left"
  [2]=>
  string(5) "right"
}
