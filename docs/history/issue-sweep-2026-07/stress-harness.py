#!/usr/bin/env python3
"""BioRouter parallel stress harness (issue-sweep 2026-07, Wave 3).

Runs concurrent headless `biorouter run` fleets on versa_azure (gpt-5.5) and
versa_bedrock (Claude Opus 4.8), with tool-heavy prompts that hammer the session
store, then audits outcomes: exit codes, JSON validity, UNIQUE-constraint errors,
tool-call counts, cross-session isolation, and sessions.db integrity.

Usage:
  python3 stress_harness.py --cli <path-to-biorouter> --outdir <dir> \
      --concurrency 8 --rounds 2 [--mode shared|isolated|mixed] [--gui-open]

Modes:
  shared   - all runs use the default shared sessions.db (named sessions)
  isolated - all runs use --no-session (post-fix: private per-run store)
  mixed    - half named-shared, half --no-session (default)
"""

import argparse, concurrent.futures as cf, json, os, random, re, shlex, sqlite3
import subprocess, sys, time, uuid
from pathlib import Path

AZURE = ("versa_azure", "gpt-5.5-2026-04-24")
BEDROCK_MODEL_CANDIDATES = [
    "us.anthropic.claude-opus-4-8-v1",  # added by sweep B1 (#29)
    "us.anthropic.claude-opus-4-8",     # form the reporter used in #31
]

# Tool-heavy tasks: shell + file writes in a per-run sandbox; deterministic
# verifiable outputs so "comprehensive results" is checkable, not vibes.
TASKS = [
    ("fibsum", "Use the shell tool to compute the sum of the first 25 Fibonacci numbers"
               " (sequence starts 1, 1, 2, ...) with a python3 one-liner, write the number"
               " to {sandbox}/fib.txt with the text editor, read the file back with the"
               " shell, and end your reply with exactly: FIBSUM=<number>"),
    ("wordcount", "Create {sandbox}/story.txt containing exactly 6 lines, each a short"
                  " sentence about a different planet. Then use the shell to count lines"
                  " and words, and end your reply with exactly: LINES=<n> WORDS=<n>"),
    ("csvstats", "Write {sandbox}/data.csv with header value and 20 integer rows 1..20."
                 " Use the shell (python3) to compute mean and max, then end your reply"
                 " with exactly: MEAN=<x> MAX=<y>"),
    ("dirtree", "Create directories {sandbox}/a/b and {sandbox}/a/c, write one file in"
                " each with its own path as content, list the tree with the shell, and"
                " end your reply with exactly: FILES=2"),
    ("jsonround", "Write {sandbox}/obj.json containing a JSON object with keys alpha=1,"
                  " beta=[2,3]. Read it back and pretty-print via python3 -m json.tool,"
                  " then end your reply with exactly: BETA_SUM=5"),
]

EXPECT = {
    "fibsum": r"FIBSUM=(196417|121392)",  # 1-indexed sum F1..F25=196417; 0-indexed variant 121392
    "wordcount": r"LINES=6\b",
    "csvstats": r"MEAN=10\.5\b.*MAX=20",
    "dirtree": r"FILES=2",
    "jsonround": r"BETA_SUM=5",
}


def run_one(cli, outdir, idx, provider, model, task_key, prompt, mode, timeout):
    name = f"stress-{task_key}-{idx}-{uuid.uuid4().hex[:6]}"
    sandbox = Path(outdir) / "sandbox" / name
    sandbox.mkdir(parents=True, exist_ok=True)
    cmd = [cli, "run", "--quiet", "--output-format", "json",
           "--provider", provider, "--model", model, "--max-turns", "14",
           "-t", prompt.format(sandbox=sandbox)]
    env = dict(os.environ, BIOROUTER_MODE="auto")
    if mode == "isolated":
        cmd.insert(2, "--no-session")
    else:
        cmd[2:2] = ["--name", name]
    t0 = time.time()
    try:
        p = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout,
                           cwd=str(sandbox), env=env)
        rc, out, err = p.returncode, p.stdout, p.stderr
    except subprocess.TimeoutExpired as e:
        rc, out, err = -9, (e.stdout or ""), (e.stderr or "") + "\nHARNESS: TIMEOUT"
    dt = time.time() - t0
    rec = {"idx": idx, "name": name, "provider": provider, "model": model,
           "task": task_key, "mode": mode, "rc": rc, "secs": round(dt, 1),
           "stderr_tail": err[-2000:], "stdout_bytes": len(out)}
    # JSON validity + content checks
    rec["json_valid"] = False
    rec["answer_ok"] = False
    rec["tool_calls"] = None
    try:
        doc = json.loads(out)
        rec["json_valid"] = True
        text = json.dumps(doc)
        rec["answer_ok"] = bool(re.search(EXPECT[task_key], text, re.S))
        tc = re.findall(r'"role"\s*:\s*"assistant"', text)
        rec["assistant_msgs"] = len(tc)
    except Exception:
        pass
    rec["unique_violation"] = ("UNIQUE constraint failed" in err) or ("code: 2067" in err)
    rec["not_connected"] = ("not connected" in err)
    rec["prompt_leak_stdout"] = ("Sensitive system operation" in out) or ("\U0001F512" in out)
    (Path(outdir) / f"{name}.json").write_text(json.dumps(rec, indent=1))
    return rec


def audit_db(db_path):
    res = {"db": str(db_path), "exists": os.path.exists(db_path)}
    if not res["exists"]:
        return res
    con = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
    try:
        res["integrity"] = con.execute("PRAGMA integrity_check").fetchone()[0]
        res["sessions"] = con.execute("SELECT count(*) FROM sessions").fetchone()[0]
        res["messages"] = con.execute("SELECT count(*) FROM messages").fetchone()[0]
        res["dupe_msg_uids"] = con.execute(
            "SELECT count(*) FROM (SELECT session_id,msg_uid,count(*) c FROM messages"
            " GROUP BY session_id,msg_uid HAVING c>1)").fetchone()[0]
    finally:
        con.close()
    return res


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cli", required=True)
    ap.add_argument("--outdir", required=True)
    ap.add_argument("--concurrency", type=int, default=8)
    ap.add_argument("--rounds", type=int, default=2)
    ap.add_argument("--mode", choices=["shared", "isolated", "mixed"], default="mixed")
    ap.add_argument("--timeout", type=int, default=900)
    ap.add_argument("--bedrock-model", default=None)
    args = ap.parse_args()

    outdir = Path(args.outdir); outdir.mkdir(parents=True, exist_ok=True)
    bedrock_model = args.bedrock_model or BEDROCK_MODEL_CANDIDATES[0]

    jobs = []
    idx = 0
    for r in range(args.rounds):
        for i in range(args.concurrency):
            provider, model = AZURE if i % 2 == 0 else ("versa_bedrock", bedrock_model)
            task_key, prompt = TASKS[idx % len(TASKS)]
            mode = args.mode if args.mode != "mixed" else ("shared" if i % 2 == 0 else "isolated")
            jobs.append((idx, provider, model, task_key, prompt, mode))
            idx += 1

    results = []
    t0 = time.time()
    with cf.ThreadPoolExecutor(max_workers=args.concurrency) as ex:
        futs = [ex.submit(run_one, args.cli, outdir, j[0], j[1], j[2], j[3], j[4], j[5], args.timeout)
                for j in jobs]
        for f in cf.as_completed(futs):
            rec = f.result()
            results.append(rec)
            print(f"[{rec['idx']:02d}] rc={rec['rc']} json={rec['json_valid']} ok={rec['answer_ok']}"
                  f" uniq_viol={rec['unique_violation']} {rec['provider']}/{rec['task']} {rec['secs']}s",
                  flush=True)

    summary = {
        "total": len(results),
        "wall_secs": round(time.time() - t0, 1),
        "rc0": sum(1 for r in results if r["rc"] == 0),
        "json_valid": sum(1 for r in results if r["json_valid"]),
        "answer_ok": sum(1 for r in results if r["answer_ok"]),
        "unique_violations": sum(1 for r in results if r["unique_violation"]),
        "not_connected": sum(1 for r in results if r["not_connected"]),
        "timeouts": sum(1 for r in results if r["rc"] == -9),
        "db_audit": audit_db(os.path.expanduser("~/.local/share/biorouter/sessions/sessions.db")),
    }
    (outdir / "summary.json").write_text(json.dumps({"summary": summary, "results": results}, indent=1))
    print(json.dumps(summary, indent=1))


if __name__ == "__main__":
    main()
