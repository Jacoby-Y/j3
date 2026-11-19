A headless program that POSTs JSON over https from a worker thread while a loop that stands in
for the frame thread keeps running and picks the reply up with `tryPop`. Point it at a URL that
echoes JSON (`j3c run -- https://postman-echo.com/post`), or change it into your API call.
`examples/openrouter-chat` is a full one: an LLM chat request, token streaming, and curl.

The pieces:
- `j3.net.http`: `http.get(url, &arena)` / `http.post(url, body, &arena)` make a `Request`;
  `req.header(name, value)` adds headers; `http.request(&req, &arena)` returns a `Response`
  (`status`, `bodyStr()`, `header(name)`), any status counts as `Ok`. `http.open` returns a
  `Stream` instead, read with `read(buf)` or `readLine(&text)` as the body arrives (server-sent
  events, `data: {...}` lines). Errors are `HttpError`; `http.errorName(e)` names one.
- `https://` goes through `j3.net.tls` on the system OpenSSL (`libssl.so.3`, loaded on first use).
  The certificate and host name are always checked. On `HttpError.Tls`, `tls.lastError(&text)`
  says why (`certificate has expired`, `hostname mismatch`). Android: the APK carries its own
  OpenSSL (run `tools/android/build-openssl.sh` once) and trusts the phone's CA store.
- `j3.data.json`: `json.parse(bytes, &arena)` gives a `&Json`; `get`, `at`, `path("a.0.b")`
  never fail (a missing value is null, `asStr()` is ""). Build with `json.obj` / `arr` / `str` /
  `num` / `boolean` plus `set` / `push`, then `json.stringify`.
- Every call blocks (connect, TLS, the reply), up to `req.timeoutMs` per step. Never call it on the
  frame thread. Give the worker its own `Arena` and everything it needs in one struct, as in
  `j3c doc recipes threads`, and take the result back through a `Queue` with `tryPop`.
- A string a request keeps (a header value, the body) must outlive the call. A backtick string
  (`Bearer {key}`) lives in the stack frame of the function that made it, so build kept strings
  with `Text` on an arena.
- Lower level: `j3.net.tcp` (`dial`, `listen`), `j3.net.dns` (`resolve`). Another program instead
  of a library: `j3.proc.run(["curl", ...], stdin, &arena)` returns its exit code, stdout, and stderr.
- Linux for now; on Windows and macOS these calls compile and return errors.
