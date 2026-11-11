"""The JSON-RPC framing the test plugins share: `Content-Length` headers, then a JSON body."""
import json
import sys


def read():
    length = None
    while True:
        line = sys.stdin.buffer.readline()
        if not line:
            return None
        line = line.strip()
        if not line:
            break
        name, _, value = line.decode().partition(":")
        if name.strip().lower() == "content-length":
            length = int(value)
    return json.loads(sys.stdin.buffer.read(length))


def send(message):
    body = json.dumps(dict(message, jsonrpc="2.0")).encode()
    sys.stdout.buffer.write(b"Content-Length: %d\r\n\r\n%s" % (len(body), body))
    sys.stdout.buffer.flush()


def serve(answer):
    """Answers each request with `answer(method, params)` until `shutdown` or the host leaves."""
    while True:
        message = read()
        if message is None:
            return
        if "method" in message and "id" in message:
            send({"id": message["id"], "result": answer(message["method"], message.get("params") or {})})
        if message.get("method") == "shutdown":
            return
