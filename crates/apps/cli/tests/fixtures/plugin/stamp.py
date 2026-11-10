"""An auth plugin, standard library only: `auth/credentials` answers with a header built from
the auth definition's config, a session cookie, and how many times it was asked."""
import json
import sys

calls = 0


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


def credentials(params):
    global calls
    calls += 1
    config = params.get("config") or {}
    return {
        "headers": {"x-stamp": config["tenant"] + ":" + config["key"], "x-calls": str(calls)},
        "cookies": {"session": "s-" + params["auth"]},
    }


def answer(method, params):
    if method == "auth/credentials":
        return credentials(params)
    if method == "initialize":
        return {}
    return None


def main():
    while True:
        message = read()
        if message is None:
            return
        if "method" in message and "id" in message:
            send({"id": message["id"], "result": answer(message["method"], message.get("params") or {})})
        if message.get("method") == "shutdown":
            return


main()
