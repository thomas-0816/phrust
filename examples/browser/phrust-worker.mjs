import { _setArgs, _setStdout, _setStderr, _setEnv } from './node_modules/@bytecodealliance/preview2-shim/dist/browser/cli.js';
import { run } from './phrust-php-worker.js';

self.postMessage({ type: 'ready' });

self.onmessage = async function(e) {
  const { code } = e.data;
  let out = '';
  let err = '';

  _setStdout({ write(bytes) { out += new TextDecoder().decode(bytes); }, flush() {} });
  _setStderr({ write(bytes) { err += new TextDecoder().decode(bytes); }, flush() {} });
  _setArgs(['phrust-php', '-r', code]);
  _setEnv({});

  try {
    const result = run.run();
    if (result instanceof Promise) await result;
    self.postMessage({ type: 'result', out, err, error: null });
  } catch (ex) {
    self.postMessage({ type: 'result', out, err, error: ex.message });
  }
};
