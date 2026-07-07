// Worker-based approach disabled: browsers don't propagate import maps to workers,
// so phrust-php.js (which uses bare specifiers internally) cannot be imported here.
// Using a simple fixed-pool cache-busting URL scheme on the main thread instead.
self.postMessage({ stdout: '', stderr: '', error: 'Worker mode unavailable (no import map propagation)' });
