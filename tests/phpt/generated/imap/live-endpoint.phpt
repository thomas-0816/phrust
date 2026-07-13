--TEST--
imap live endpoint smoke through Rust IMAP backend
--SKIPIF--
<?php
if (!extension_loaded("imap")) die("skip imap extension not loaded");
if (getenv("PHRUST_NET_TESTS") !== "1") die("skip PHRUST_NET_TESTS=1 not set");
if (getenv("PHRUST_IMAP_LIVE_ENDPOINT") === false) die("skip PHRUST_IMAP_LIVE_ENDPOINT not set");
if (getenv("PHRUST_IMAP_LIVE_MAILBOX") === false) die("skip PHRUST_IMAP_LIVE_MAILBOX not set");
if (getenv("PHRUST_IMAP_LIVE_USER") === false) die("skip PHRUST_IMAP_LIVE_USER not set");
if (getenv("PHRUST_IMAP_LIVE_PASSWORD") === false) die("skip PHRUST_IMAP_LIVE_PASSWORD not set");
?>
--FILE--
<?php
$mailbox = getenv("PHRUST_IMAP_LIVE_MAILBOX");
$user = getenv("PHRUST_IMAP_LIVE_USER");
$password = getenv("PHRUST_IMAP_LIVE_PASSWORD");
$criteria = getenv("PHRUST_IMAP_LIVE_SEARCH") ?: "ALL";

$imap = imap_open($mailbox, $user, $password);
var_dump($imap instanceof IMAP\Connection);

$check = imap_check($imap);
var_dump(is_object($check));
var_dump(is_int($check->Nmsgs));

$info = imap_mailboxmsginfo($imap);
var_dump(is_object($info));
var_dump(is_int($info->Nmsgs));

$headers = imap_headers($imap);
var_dump(is_array($headers));

$search = imap_search($imap, $criteria);
var_dump($search === false || is_array($search));

$header = imap_fetchheader($imap, 1);
var_dump($header === false || is_string($header));

$body = imap_fetchbody($imap, 1, "1");
var_dump($body === false || is_string($body));

$errors = imap_errors();
var_dump($errors === false || is_array($errors));
var_dump(imap_close($imap));
?>
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
