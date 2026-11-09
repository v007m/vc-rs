"""A reporter plugin, standard library only: it counts the run's events, reads the secret its
manifest declares, is refused one it does not, and returns one report file."""
import json
import sys

events = []


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


def secret(name):
    send({"id": "s-" + name, "method": "secrets/get", "params": {"name": name}})
    while True:
        message = read()
        if message is None or message.get("id") == "s-" + name:
            return message or {"error": {"code": -32603, "message": "the host left"}}


def report(summary):
    token = secret("reportToken")
    refused = secret("apiKey")
    lines = [
        "status " + summary["status"],
        "events " + str(len(events)),
        "reportToken " + str(len(token.get("result", {}).get("value", ""))) + " chars",
        "apiKey " + ("refused " + str(refused["error"]["code"]) if "error" in refused else "given"),
    ]
    return {"files": [{"name": "plugin/report.txt", "content": "\n".join(lines) + "\n"}]}


def answer(method, params):
    if method == "reporter/onEvent":
        events.append(params["event"]["event"])
        return None
    if method == "reporter/finish":
        return report(params["summary"])
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
