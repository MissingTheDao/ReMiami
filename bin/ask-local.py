#!/usr/bin/env python3
"""Send a coding task to the local llama-server (OpenAI-compatible) and print the reply.

usage: ask-local.py [-f context_file ...] [-o out_file] [--code] [--max-tokens N] "task prompt"
  -f      attach file(s) as context (repeatable)
  -o      write the reply to a file instead of stdout
  --code  ask for code only and strip a surrounding ``` fence from the reply
Env: LOCAL_AI_URL (default http://127.0.0.1:8091)
"""
import argparse, json, os, re, sys, time, urllib.request

ap = argparse.ArgumentParser()
ap.add_argument('prompt')
ap.add_argument('-f', action='append', default=[])
ap.add_argument('-o')
ap.add_argument('--code', action='store_true')
ap.add_argument('--max-tokens', type=int, default=8192)
a = ap.parse_args()

ctx = ''.join(f'\n\n=== {p} ===\n{open(p, encoding="utf-8", errors="replace").read()}' for p in a.f)
system = 'You are a careful senior C++/Rust engineer. Follow the task exactly.'
if a.code:
    system += ' Reply with only the requested code in one fenced block, no explanation.'
body = {
    'messages': [{'role': 'system', 'content': system},
                 {'role': 'user', 'content': a.prompt + ctx}],
    'max_tokens': a.max_tokens,
    'chat_template_kwargs': {'enable_thinking': False},
}
url = os.environ.get('LOCAL_AI_URL', 'http://127.0.0.1:8091') + '/v1/chat/completions'
t = time.time()
req = urllib.request.Request(url, json.dumps(body).encode(), {'Content-Type': 'application/json'})
r = json.load(urllib.request.urlopen(req, timeout=1800))
out = r['choices'][0]['message']['content']
if a.code:
    m = re.search(r'```[\w+-]*\n(.*?)```', out, re.S)
    if m: out = m.group(1)
u = r.get('usage', {})
print(f'[local-ai] {u.get("prompt_tokens")}+{u.get("completion_tokens")} tok in {time.time()-t:.0f}s', file=sys.stderr)
if a.o: open(a.o, 'w').write(out)
else: print(out)
