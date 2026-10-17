#!/usr/bin/env python3
"""Local RPC fixture: real file reads and fixed shell commands in a disposable project.

This is a verification tool, not a Pi replacement. Use only with an isolated
GAP_DATA_DIR and fixture projects containing one.md, two.md and three.md.
"""
import argparse
import copy
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import threading
import time


class Replay:
    def __init__(self, session):
        self.path = Path(str(session) + ".replay.json")
        self.messages = json.loads(self.path.read_text()) if self.path.exists() else []
        self.turn = len([m for m in self.messages if m["role"] == "user"])
        self.lock = threading.Lock()
        self.output = threading.Lock()
        self.stop = threading.Event()
        self.worker = None
        self.busy = False

    def emit(self, value):
        with self.output:
            print(json.dumps(value, ensure_ascii=False), flush=True)

    def event(self, kind, **fields):
        self.emit(dict(type=kind, **fields))

    def response(self, request, data=None, error=None):
        self.emit(dict(type="response", id=request["id"], command=request["type"],
                       success=error is None, data=data or {}, error=error))

    def snapshot(self):
        with self.lock:
            return copy.deepcopy(self.messages)

    def message(self, value):
        self.event("message_start", message=value)
        self.stop.wait(.12)
        self.event("message_end", message=value)
        with self.lock:
            self.messages.append(value)

    def tool(self, call, name, arguments, text, error=False):
        self.event("tool_execution_end", toolCallId=call, toolName=name,
                   result={"content": [{"type": "text", "text": text}]}, isError=error)
        self.stop.wait(.12)
        self.message(dict(role="toolResult", toolCallId=call, toolName=name,
                          content=[{"type": "text", "text": text}],
                          isError=error, timestamp=self.turn * 100 + len(self.snapshot())))

    def run(self, prompt):
        try:
            self.event("agent_start")
            self.message(dict(role="user", content=prompt, timestamp=self.turn * 100))
            # Identical IDs across separate processes deliberately exercise session scoping.
            self.message(dict(role="assistant", timestamp=self.turn * 100 + 1,
                              content=[{"type": "thinking", "thinking": "Fixture planning"}]))
            for ix, filename in enumerate(["one.md", "two.md", "three.md"]):
                if self.stop.wait(.35):
                    return
                call = f"turn-{self.turn}-read-{ix}"
                arguments = {"path": filename}
                self.event("tool_execution_start", toolCallId=call, toolName="read", args=arguments)
                text = Path(filename).read_text()
                self.tool(call, "read", arguments, text)
            if self.stop.is_set():
                return
            call = f"turn-{self.turn}-bash"
            command = ("printf 'long-command-started\\n'; sleep 30" if "stop" in prompt.lower()
                       else "printf 'command-started\\n'; sleep 1; printf '中文输出\\n'; "
                            "sleep 1; printf 'intentional failure\\n'; exit 17")
            self.event("tool_execution_start", toolCallId=call, toolName="bash", args={"command": command})
            process = subprocess.Popen(["/bin/sh", "-c", command], stdout=subprocess.PIPE,
                                       stderr=subprocess.STDOUT, start_new_session=True)
            chunks = []
            # Poll on a separate reader so cancellation can reap a quiet sleeping command.
            def read_output():
                for line in process.stdout:
                    chunks.append(line.decode("utf-8"))
                    self.event("tool_execution_update", toolCallId=call, toolName="bash",
                               partialResult={"content": [{"type": "text", "text": "".join(chunks)}]})
            reader = threading.Thread(target=read_output)
            reader.start()
            try:
                while process.poll() is None:
                    if self.stop.wait(.04):
                        try:
                            os.killpg(process.pid, signal.SIGKILL)
                        except ProcessLookupError:
                            pass
                        break
            finally:
                process.wait()
                reader.join()
                process.stdout.close()
            text = "".join(chunks) + ("Stopped" if self.stop.is_set() else f"Exit code: {process.returncode}")
            self.tool(call, "bash", {"command": command}, text, process.returncode != 0)
            if not self.stop.is_set():
                self.message(dict(role="assistant", timestamp=self.turn * 100 + 99,
                                  content=[{"type": "text", "text": "Read three files. Command failure recorded."}]))
        finally:
            self.path.write_text(json.dumps(self.snapshot(), ensure_ascii=False))
            with self.lock:
                self.busy = False
            self.event("agent_settled")

    def request(self, request):
        kind = request["type"]
        if kind == "get_messages":
            self.response(request, {"messages": self.snapshot()})
        elif kind == "get_state":
            self.response(request, {"isStreaming": self.busy, "model": {"id": "local-tool-replay"}, "thinkingLevel": "off"})
        elif kind in ["get_available_models", "get_commands", "get_available_thinking_levels"]:
            key = {"get_available_models": "models", "get_commands": "commands", "get_available_thinking_levels": "levels"}[kind]
            self.response(request, {key: []})
        elif kind == "prompt":
            if self.busy:
                self.response(request, error="Fixture is already running")
                return
            self.turn += 1
            self.busy = True
            self.stop.clear()
            self.response(request, {"disposition": "started"})
            self.worker = threading.Thread(target=self.run, args=(request["message"],))
            self.worker.start()
        elif kind == "abort":
            self.stop.set()
            self.response(request)
        else:
            self.response(request)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--mode")
    parser.add_argument("--session", required=True)
    args = parser.parse_args()
    replay = Replay(args.session)
    try:
        for line in sys.stdin:
            replay.request(json.loads(line))
    finally:
        replay.stop.set()
        if replay.worker:
            replay.worker.join()


if __name__ == "__main__":
    main()
