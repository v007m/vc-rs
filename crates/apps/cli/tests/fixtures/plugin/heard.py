"""A reporter plugin, standard library only: it writes down the method of each message the host
sends it, then `EOF` once its stdin closes, to `heard.log` in its data folder. It answers each
request and keeps reading after `shutdown`, so the log shows whether the host closed stdin."""
import os

from rpc import read, send

log = open(os.path.join(os.environ["VIB_PLUGIN_DATA"], "heard.log"), "a")

while True:
    message = read()
    if message is None:
        log.write("EOF\n")
        log.close()
        break
    method = message.get("method", "answer")
    log.write(method + "\n")
    log.flush()
    if "id" in message and "method" in message:
        result = {"files": []} if method == "reporter/finish" else None
        send({"id": message["id"], "result": result})
