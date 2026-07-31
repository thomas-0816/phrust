<?php

$stream = fopen('php://memory', 'w+');
echo 'write=', fwrite($stream, "alpha\nbeta\n"), "\n";
echo 'tell_after_write=', ftell($stream), "\n";
echo 'rewind=', rewind($stream) ? '1' : '0', "\n";
echo 'read=', fread($stream, 5), "\n";
echo 'getc=', bin2hex(fgetc($stream)), "\n";
echo 'gets=', bin2hex(fgets($stream)), "\n";
echo 'eof=', feof($stream) ? '1' : '0', "\n";

rewind($stream);
echo 'bounded_gets=', fgets($stream, 3), "\n";
echo 'tell_after_gets=', ftell($stream), "\n";
echo 'seek_end=', fseek($stream, -5, SEEK_END), "\n";
echo 'tail=', bin2hex(fread($stream, 5)), "\n";
echo 'remaining=', stream_get_contents($stream, 4, 6), "\n";
echo 'tell_after_remaining=', ftell($stream), "\n";
echo 'flush=', fflush($stream) ? '1' : '0', "\n";
echo 'truncate=', ftruncate($stream, 6) ? '1' : '0', "\n";
rewind($stream);
echo 'truncated=', bin2hex(stream_get_contents($stream)), "\n";

$destination = fopen('php://memory', 'w+');
rewind($stream);
echo 'copied=', stream_copy_to_stream($stream, $destination, -1, 0), "\n";
rewind($destination);
echo 'copy_contents=', bin2hex(stream_get_contents($destination)), "\n";

rewind($stream);
try {
    fread($stream, 0);
} catch (ValueError $error) {
    echo 'fread_value_error=', get_class($error), "\n";
}
echo 'tell_after_fread_error=', ftell($stream), "\n";
try {
    fgets($stream, 0);
} catch (ValueError $error) {
    echo 'fgets_value_error=', get_class($error), "\n";
}
echo 'tell_after_fgets_error=', ftell($stream), "\n";
try {
    stream_get_contents($stream, -2);
} catch (ValueError $error) {
    echo 'contents_value_error=', get_class($error), "\n";
}
echo 'tell_after_contents_error=', ftell($stream), "\n";

echo 'close_destination=', fclose($destination) ? '1' : '0', "\n";
echo 'close_stream=', fclose($stream) ? '1' : '0', "\n";
