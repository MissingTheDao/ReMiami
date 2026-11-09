#!/usr/bin/env python3
"""Send a coding task to the local llama-server (OpenAI-compatible) and print the reply.

usage: ask-local.py [-f context_file ...] [-o out_file] [--code] [--max-tokens N]
                    [--ollama MODEL] "task prompt"
  -f        attach file(s) as context (repeatable)
  -o        write the reply to a file instead of stdout
  --code    ask for code only and strip a surrounding ``` fence from the reply
  --ollama  use this Ollama model (native API, thinking off) instead of llama-server
Env: LOCAL_AI_URL (default http://127.0.0.1:8091, or http://127.0.0.1:11434 with --ollama),
     LOCAL_AI_OLLAMA_MODEL (default model for --ollama when given without a value)
"""
import argparse, json, os, re, sys, time, urllib.request

ap = argparse.ArgumentParser()
ap.add_argument('prompt')
ap.add_argument('-f', action='append', default=[])
ap.add_argument('-o')
ap.add_argument('--code', action='store_true')
ap.add_argument('--max-tokens', type=int, default=8192)
ap.add_argument('--ollama', nargs='?', const=os.environ.get('LOCAL_AI_OLLAMA_MODEL', ''), default=None)
a = ap.parse_args()

ctx = ''.join(f'\n\n=== {p} ===\n{open(p, encoding="utf-8", errors="replace").read()}' for p in a.f)
system = 'You are a careful senior C++/Rust engineer. Follow the task exactly.'
if a.code:
    system += ' Reply with only the requested code in one fenced block, no explanation.'
messages = [{'role': 'system', 'content': system},
            {'role': 'user', 'content': a.prompt + ctx}]
t = time.time()
if a.ollama is not None:
    if not a.ollama:
        sys.exit('--ollama needs a model name (or set LOCAL_AI_OLLAMA_MODEL)')
    # Same sampling as bin/ai-serve.sh; 32k context like the llama-server setup.
    body = {'model': a.ollama, 'messages': messages, 'stream': False, 'think': False,
            'options': {'temperature': 0.6, 'top_p': 0.95, 'top_k': 20, 'min_p': 0.0,
                        'num_predict': a.max_tokens, 'num_ctx': 32768}}
    url = os.environ.get('LOCAL_AI_URL', 'http://127.0.0.1:11434') + '/api/chat'
    req = urllib.request.Request(url, json.dumps(body).encode(), {'Content-Type': 'application/json'})
    r = json.load(urllib.request.urlopen(req, timeout=1800))
    out = r['message']['content']
    usage = (r.get('prompt_eval_count'), r.get('eval_count'))
else:
    body = {'messages': messages, 'max_tokens': a.max_tokens,
            'chat_template_kwargs': {'enable_thinking': False}}
    url = os.environ.get('LOCAL_AI_URL', 'http://127.0.0.1:8091') + '/v1/chat/completions'
    req = urllib.request.Request(url, json.dumps(body).encode(), {'Content-Type': 'application/json'})
    r = json.load(urllib.request.urlopen(req, timeout=1800))
    out = r['choices'][0]['message']['content']
    u = r.get('usage', {})
    usage = (u.get('prompt_tokens'), u.get('completion_tokens'))
if a.code:
    m = re.search(r'```[\w+-]*\n(.*?)```', out, re.S)
    if m: out = m.group(1)
    elif out.lstrip().startswith('```'):  # reply cut off before the closing fence
        out = out.lstrip().partition('\n')[2]
print(f'[local-ai] {usage[0]}+{usage[1]} tok in {time.time()-t:.0f}s', file=sys.stderr)
if a.o: open(a.o, 'w').write(out)
else: print(out)
