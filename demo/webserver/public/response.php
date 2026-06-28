<?php
require "lib/page.php";

header("X-Phrust-Demo: response");
http_response_code(202);

demo_title("Headers and status");

echo "<p>This PHP script sets <code>HTTP 202</code> and an <code>X-Phrust-Demo</code> response header before writing the page body.</p>\n";
echo "<p>Check it with:</p>\n";
echo "<pre>curl -i http://127.0.0.1:8080/response.php</pre>\n";

demo_footer();
