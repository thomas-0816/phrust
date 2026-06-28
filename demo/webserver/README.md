# phrust webserver demo

This demo serves PHP files through the in-process `phrust-server`.

Start it from the repository root:

```bash
nix develop -c demo/webserver/start.sh
```

Then open:

```text
http://127.0.0.1:8080/
```

The demo pages are all PHP files under `public/` and exercise request
superglobals, functions, includes, loops, arrays, response headers, and POST
form handling.
