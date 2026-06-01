#!/usr/bin/env python3
"""Publish a Plan event to Kafka using kafka-python.

Usage:
  python3 seed_plan.py [PRESET]
  python3 seed_plan.py --stdin

Presets:
  hello   (default)  hello-world plan with action "greet"
  health             system-health plan with action "report"
"""

import argparse
import json
import os
import random
import sys
import time
from datetime import datetime

try:
    from kafka import KafkaAdminClient, KafkaProducer
    from kafka.admin import NewTopic
    from kafka.errors import TopicAlreadyExistsError
except ImportError as exc:
    raise SystemExit(
        "kafka-python is required. Install it with: python3 -m pip install kafka-python"
    ) from exc


def die(message: str) -> None:
    print(f"[seed_plan] ERROR: {message}", file=sys.stderr)
    sys.exit(1)


def warn(message: str) -> None:
    print(f"[seed_plan] WARNING: {message}", file=sys.stderr)


def log(message: str) -> None:
    print(f"[seed_plan] {message}")


def gen_id() -> str:
    try:
        import ulid

        return str(ulid.new())
    except Exception:
        return f"task-{int(time.time())}-{random.randint(0, 999999):06d}"


def format_time() -> str:
    return datetime.utcnow().strftime("%Y-%m-%dT%H:%M:%SZ")


def load_preset_plan(preset: str) -> dict:
    if preset == "hello":
        plan_id = gen_id()
        task_id = gen_id()
        return {
            "id": plan_id,
            "name": "hello-world",
            "description": "Simple greeting plan to verify the generic watcher is running.",
            "action": "greet",
            "version": "1.0.0",
            "goals": ["Confirm the generic watcher received and executed this plan"],
            "tasks": [
                {
                    "id": task_id,
                    "description": (
                        "You are XZatoma running in generic watcher mode. Run these three commands and report each one with its output: "
                        "(1) echo Generic watcher is alive (2) date -u (3) uname -s. Then run mkdir -p tmp and write a brief report to tmp/hello-world-report.txt containing: "
                        "a header line XZatoma Generic Watcher - Hello World, the timestamp from command 2, and the platform from command 3. Finish with cat tmp/hello-world-report.txt to confirm the file was written."
                    ),
                    "priority": "low",
                }
            ],
            "max_iterations": 5,
            "allow_dangerous": False,
            "result_mentions": ["tmp/hello-world-report.txt"],
        }

    if preset == "health":
        plan_id = gen_id()
        task_id_1 = gen_id()
        task_id_2 = gen_id()
        return {
            "id": plan_id,
            "name": "system-health",
            "description": "Basic system health check - runs diagnostics and writes a report file.",
            "action": "report",
            "version": "1.0.0",
            "goals": ["Collect and report current system health metrics"],
            "tasks": [
                {
                    "id": task_id_1,
                    "description": (
                        "Gather system information by running each of the following commands and reporting the command name and its output clearly: uname -a, date -u, df -h ."
                    ),
                    "priority": "high",
                },
                {
                    "id": task_id_2,
                    "description": (
                        "Write a plain-text system health report to ./tmp/system-health-report.txt. First run mkdir -p ./tmp to ensure the directory exists. "
                        "The report must contain: a header line System Health Report, a timestamp from date -u, the platform info from uname -a, a disk usage section from df -h ., and a footer line end of report. "
                        "After writing confirm the file exists with head -3 tmp/system-health-report.txt."
                    ),
                    "priority": "medium",
                },
            ],
            "max_iterations": 8,
            "allow_dangerous": False,
            "result_mentions": ["tmp/system-health-report.txt"],
        }
    if preset == "audit":
        plan_id = gen_id()
        task_id_1 = gen_id()
        task_id_2 = gen_id()
        return {
            "id": plan_id,
            "name": "doc-comment-audit",
            "description": "Demonstrates multi-task subagent delegation delivered via the generic Kafka watcher. A coordinator plan prepares a tmp directory, delegates a doc-comment audit of the src/ directory to one worker subagent, delegates report writing to a second worker subagent, and then verifies the generated report.\n",
            "action": "audit",
            "version": "1.0.0",
            "max_iterations": 20,
            "allow_dangerous": False,
            "goals": [
                "Use the subagent tool to delegate a code audit task to a worker agent.",
            ],
            "result_mentions": ["tmp/xzatoma-audit-report.md"],
            "tasks": [
                {
                    "id": "ensure_tmp_dir",
                    "description": 'Use the terminal tool to prepare the demo-local tmp directory and confirm\nthat it exists.\n\nRun these commands in order and report their outputs:\n\n1. mkdir -p tmp\n2. test -d tmp && echo "exists" || echo "missing"\n\nReport the results in this exact format:\n\nCOMMAND: <command>\nOUTPUT:  <output or empty string on success>\n\nIf the directory check reports "missing", stop and explain the failure.\n',
                    "priority": "medium",
                    "dependencies": [],
                },
                {
                    "id": "audit",
                    "description": 'Use the subagent tool to delegate a code audit task to a worker agent.\n\nThe worker\'s job is to find public Rust functions in the src/ directory\nthat are missing doc comments.\n\nInvoke the subagent tool with these exact parameters:\n\n  label: "doc-audit-worker"\n  task_prompt: |\n    You are a code audit worker. Your job is to find public Rust\n    functions in the src/ directory that are missing doc comments.\n\n    A public function is missing a doc comment when a line matching\n    the pattern `pub fn` is NOT immediately preceded by a line\n    starting with `///`.\n\n    Steps:\n    1. Use the list_directory tool to list the files under src/.\n    2. For each .rs file found, use the grep tool to find all lines\n       containing `pub fn`.\n    3. For each matching line, use read_file to inspect the line\n       immediately before the `pub fn` line. Record the function name\n       and file path for every `pub fn` that is NOT preceded by a\n       `///` doc comment line.\n    4. Produce a plain-text list in this format, one entry per line:\n         MISSING: <file>:<line_number>  <function_signature>\n    5. At the end of the list print a summary line:\n         TOTAL MISSING: <count>\n\n    Audit scope: src/ directory only. Do not recurse into target/ or\n    any hidden directories.\n  summary_prompt: >\n    Summarise the audit findings as a structured list of file paths and\n    function signatures that are missing doc comments, followed by the\n    total count.\n  allowed_tools:\n    - grep\n    - read_file\n    - list_directory\n  max_turns: 10\n\nWait for the subagent to complete and then report the worker output in\nfull. Do not summarise, trim, or rewrite it. Preserve every MISSING:\nline and the final TOTAL MISSING: line exactly so later steps can rely\non the complete text.\n',
                    "priority": "high",
                    "dependencies": ["ensure_tmp_dir"],
                },
                {
                    "id": "summarise",
                    "description": 'Use the subagent tool to delegate writing a Markdown summary report to a\nsecond worker agent.\n\nThe full audit output from the previous task is available in the\nconversation context. Pass that output to the worker exactly as returned.\n\nInvoke the subagent tool with these exact parameters:\n\n  label: "report-writer-worker"\n  task_prompt: |\n    You are a report-writing worker. Your job is to write a Markdown\n    summary report of a doc-comment audit.\n\n    The raw audit output you must summarise is provided below between\n    the BEGIN AUDIT OUTPUT and END AUDIT OUTPUT markers.\n\n    BEGIN AUDIT OUTPUT\n    <paste the full audit output from the previous task here>\n    END AUDIT OUTPUT\n\n    Steps:\n    1. Parse the audit output. Each line beginning with MISSING:\n       records one public function that lacks a doc comment. The final\n       line beginning with TOTAL MISSING: gives the count.\n    2. Write a Markdown report to the file tmp/xzatoma-audit-report.md.\n\n       The report must contain these sections in order:\n\n       # XZatoma Doc-Comment Audit Report\n\n       A brief one-paragraph introduction explaining what was audited\n       and why doc comments matter.\n\n       ## Summary\n\n       A table with two columns: File and Count. Each row lists one\n       source file and the number of undocumented public functions found\n       in that file. Include a totals row at the bottom.\n\n       ## Missing Doc Comments\n\n       A bullet list of every missing entry in the format:\n         - `<file>:<line_number>` - `<function_signature>`\n\n       ## Recommendations\n\n       Three or four concrete recommendations for improving doc comment\n       coverage.\n\n       ## Footer\n\n       The line: "Report generated by XZatoma subagent demo."\n\n    3. Use the write_file tool to write the report to\n       tmp/xzatoma-audit-report.md.\n    4. After writing, use read_file to read the file back and confirm\n       it was written successfully by reporting the first five lines.\n  allowed_tools:\n    - write_file\n    - read_file\n  max_turns: 5\n\nWait for the subagent to complete and then report whether the file was\nwritten successfully. Include the first five lines the worker read back\nfrom tmp/xzatoma-audit-report.md.\n',
                    "priority": "high",
                    "dependencies": ["audit"],
                },
                {
                    "id": "verify",
                    "description": "Verify that the report writer subagent created the expected Markdown file\nat tmp/xzatoma-audit-report.md.\n\nPerform these checks using the read_file tool:\n\n1. Read tmp/xzatoma-audit-report.md.\n2. Confirm that the file begins with this heading:\n     # XZatoma Doc-Comment Audit Report\n3. Confirm that the file contains these section headings:\n     ## Summary\n     ## Missing Doc Comments\n     ## Recommendations\n4. Print the first 20 lines of the file verbatim.\n5. State clearly whether all required sections were found.\n\nIf the file does not exist or is empty, report the failure clearly and\nstop.\n",
                    "priority": "medium",
                    "dependencies": ["summarise"],
                },
            ],
        }

    die(f"Unknown preset '{preset}'. Valid presets: hello, health, audit.")


def build_envelope(plan: dict) -> dict:
    return {
        "id": gen_id(),
        "specversion": "1.0",
        "type": "xzatoma.plan.execute",
        "source": "xzatoma.seed-plan",
        "time": format_time(),
        "datacontenttype": "application/json",
        "data": plan,
    }


def ensure_topic(admin_client: KafkaAdminClient, topic: str) -> None:
    try:
        admin_client.create_topics(
            [NewTopic(name=topic, num_partitions=1, replication_factor=1)]
        )
        log(f"Ensured topic '{topic}' exists.")
    except TopicAlreadyExistsError:
        log(f"Topic '{topic}' already exists.")
    except Exception as exc:
        warn(f"Could not create topic '{topic}': {exc}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Publish a Plan event to Kafka using kafka-python."
    )
    parser.add_argument(
        "preset",
        nargs="?",
        default="hello",
        choices=["hello", "health"],
        help="Preset plan to send",
    )
    parser.add_argument(
        "--stdin",
        action="store_true",
        help="Read a custom plan JSON document from stdin",
    )
    parser.add_argument(
        "--topic",
        default=os.environ.get("XZATOMA_TOPIC", "xzatoma.plans"),
        help="Kafka topic to publish to",
    )
    parser.add_argument(
        "--brokers",
        default=os.environ.get("XZATOMA_BROKERS", "localhost:9092"),
        help="Kafka bootstrap brokers",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()

    if args.stdin and args.preset != "hello":
        log("Ignoring positional preset when --stdin is used.")

    if args.stdin:
        raw = sys.stdin.read()
        if not raw.strip():
            die("No input received on stdin. Pipe a Plan JSON document to this script.")
        try:
            plan = json.loads(raw)
        except json.JSONDecodeError as exc:
            die(f"Plan payload is not valid JSON: {exc}")
        plan_name = plan.get("name", "(unknown)")
        plan_action = plan.get("action", "(none)")
        plan_version = plan.get("version", "(none)")
        plan_id = plan.get("id", "(none)")
    else:
        plan = load_preset_plan(args.preset)
        plan_name = plan["name"]
        plan_action = plan["action"]
        plan_version = plan.get("version", "(none)")
        plan_id = plan.get("id", "(none)")

    envelope = build_envelope(plan)
    brokers = [broker.strip() for broker in args.brokers.split(",") if broker.strip()]

    try:
        admin_client = KafkaAdminClient(bootstrap_servers=brokers)
        ensure_topic(admin_client, args.topic)
        admin_client.close()
    except Exception as exc:
        warn(f"Kafka admin client failed: {exc}")

    try:
        producer = KafkaProducer(
            bootstrap_servers=brokers,
            value_serializer=lambda value: json.dumps(value).encode("utf-8"),
        )
        metadata = producer.send(args.topic, envelope).get(timeout=10)
        producer.flush()
        producer.close()
    except Exception as exc:
        die(f"Failed to publish plan event: {exc}")

    log(f"Publishing plan event to topic '{args.topic}' ...")
    log(f"  ID      : {plan_id}")
    log(f"  Name    : {plan_name}")
    log(f"  Action  : {plan_action}")
    log(f"  Version : {plan_version}")
    log(f"  Partition: {metadata.partition}, Offset: {metadata.offset}")

    print("\nPayload published:")
    print(json.dumps(envelope, indent=2))

    print("\nNext steps:")
    print("  1. Start XZatoma in generic watcher mode if it is not running.")
    print(
        "  2. XZatoma should pick up the plan event and publish a PlanResultEvent to the output topic."
    )
    print(
        "  3. Watch the result in a separate terminal or via your Kafka monitoring tools."
    )
    print("  4. To send another event with a different preset:")
    print("       python3 seed_plan.py hello")
    print("       python3 seed_plan.py health")
    print("       python3 seed_plan.py audit")
    print("  5. To send a custom plan via stdin:")
    print(
        '       echo \'{"name":"my-plan","action":"deploy","version":"2.0.0","tasks":[{"id":"deploy-task","description":"Run: echo deployed"}]}\' | python3 seed_plan.py --stdin'
    )


if __name__ == "__main__":
    main()
