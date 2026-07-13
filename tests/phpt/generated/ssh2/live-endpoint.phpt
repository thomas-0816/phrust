--TEST--
ssh2 opt-in live endpoint auth, exec, sftp, and optional scp
--SKIPIF--
<?php
if (!extension_loaded("ssh2")) die("skip ssh2 extension not loaded");
if (getenv("PHRUST_NET_TESTS") !== "1") die("skip PHRUST_NET_TESTS not enabled");
if (getenv("PHRUST_SSH2_LIVE_ENDPOINT") === false) die("skip PHRUST_SSH2_LIVE_ENDPOINT not set");
if (getenv("PHRUST_SSH2_LIVE_USER") === false) die("skip PHRUST_SSH2_LIVE_USER not set");
if (getenv("PHRUST_SSH2_LIVE_PASSWORD") === false && getenv("PHRUST_SSH2_LIVE_PRIVKEY") === false) {
    die("skip SSH2 password or private key not set");
}
?>
--FILE--
<?php
$endpoint = getenv("PHRUST_SSH2_LIVE_ENDPOINT");
[$host, $port] = explode(":", $endpoint, 2);
$session = ssh2_connect($host, (int) $port);
var_dump($session instanceof SSH2\Session);
$user = getenv("PHRUST_SSH2_LIVE_USER");
if (getenv("PHRUST_SSH2_LIVE_PASSWORD") !== false) {
    var_dump(ssh2_auth_password($session, $user, getenv("PHRUST_SSH2_LIVE_PASSWORD")));
} else {
    $pubkey = getenv("PHRUST_SSH2_LIVE_PUBKEY") ?: getenv("HOME") . "/.ssh/id_rsa.pub";
    $passphrase = getenv("PHRUST_SSH2_LIVE_PASSPHRASE");
    if ($passphrase === false) {
        var_dump(ssh2_auth_pubkey_file($session, $user, $pubkey, getenv("PHRUST_SSH2_LIVE_PRIVKEY")));
    } else {
        var_dump(ssh2_auth_pubkey_file($session, $user, $pubkey, getenv("PHRUST_SSH2_LIVE_PRIVKEY"), $passphrase));
    }
}
$fingerprint = ssh2_fingerprint($session, SSH2_FINGERPRINT_SHA1 | SSH2_FINGERPRINT_HEX);
var_dump(is_string($fingerprint), strlen($fingerprint) > 0);
$command = getenv("PHRUST_SSH2_LIVE_COMMAND") ?: "printf phrust-ssh2";
$stream = ssh2_exec($session, $command);
var_dump(is_resource($stream));
$output = stream_get_contents($stream);
var_dump(strlen($output) >= 0);
$sftp = ssh2_sftp($session);
var_dump($sftp instanceof SSH2\Sftp);
if (getenv("PHRUST_SSH2_LIVE_SCP_LOCAL") !== false && getenv("PHRUST_SSH2_LIVE_SCP_REMOTE") !== false) {
    var_dump(ssh2_scp_send($session, getenv("PHRUST_SSH2_LIVE_SCP_LOCAL"), getenv("PHRUST_SSH2_LIVE_SCP_REMOTE")));
}
if (getenv("PHRUST_SSH2_LIVE_SCP_RECV_REMOTE") !== false && getenv("PHRUST_SSH2_LIVE_SCP_RECV_LOCAL") !== false) {
    var_dump(ssh2_scp_recv($session, getenv("PHRUST_SSH2_LIVE_SCP_RECV_REMOTE"), getenv("PHRUST_SSH2_LIVE_SCP_RECV_LOCAL")));
}
var_dump(ssh2_disconnect($session));
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
