# Spike findings — Claude Code CLI as Mozaik Participant

Date: 2026-05-03
Branch: `mozaik-rework`
Spike file: `scripts/spike-claude-participant.ts`
Logs: `scripts/spike-logs/*.jsonl` (166 events across 3 runs)

## Result: ALL acceptance + stretch criteria PASSED

| Criterion | Target | Actual |
|---|---|---|
| Distinct Claude event types observed | ≥ 5 | **10** |
| Logger captures with source identification | yes | yes |
| Claude process exits gracefully | yes | code 0 in all 3 runs |
| `spike-event-log.jsonl` written | yes | 3 files, 166 total events |
| **STRETCH** Two parallel Claudes interleave on one bus | yes | **yes — events truly interleave** |
| **STRETCH** Mid-flight bus injection accepted by Claude | unknown | **yes — Claude queues mid-flight messages and processes as additional turns** |

## Distinct Claude event types catalogued

```
system:init
system:status
system:task_started
system:task_notification
rate_limit_event
stream_event
user                  (input replay + tool result echoes)
assistant             (full assistant message)
result:success        (final, with usage/cost/session_id)
```

## Five key findings

1. **Bidirectional stream-json works as advertised.** Spawn Claude with `--print --input-format stream-json --output-format stream-json --verbose --include-partial-messages --replay-user-messages --permission-mode bypassPermissions`. Each line in is a JSON event, each line out is a JSON event. No surprises.

2. **Mid-flight injection is real.** Sending a second `user` event on stdin while Claude is in the middle of executing the first task does **not** error and does **not** get dropped. Claude processes both — `result.num_turns: 2` confirms it ran them as sequential turns within the same session. This is the architectural foundation we needed: bus participants (Critic, Sentry, Librarian) can emit user messages to a Claude participant's environment and Claude will pick them up between turns.

3. **Two Claude processes share a Mozaik bus cleanly.** Events from S1 and S2 truly interleave in real time on the logger — the bus does not serialize them. This validates that N parallel Claude participants in one `AgenticEnvironment` will give us cooperative parallelism, not just isolated parallelism.

4. **The `result` event is gold.** It carries `session_id` (for `--resume` in Phase 1 sub-turns), `total_cost_usd`, full token usage with cache breakdown, `num_turns`, `duration_ms`, and `iterations[]` per-turn token attribution. This single event already gives us most of what `Treasurer` needs and what baro currently extracts manually from stream parsing.

5. **`stream_event` is by far the most frequent type** (~80% of events with `--include-partial-messages`). For Phase 1 we will likely **filter or batch these** before delivering on the bus to keep observers from being flooded — these are partial token deltas, not semantically interesting items. The semantically meaningful events are `assistant`, `user`, `system:*`, and `result:*`.

## Implications for Phase 1 design

- **Stream-json mapper** has 10 input shapes to handle. The mapping is straightforward; no exotic format weirdness was found. ✓ low risk
- **Partial token streaming** is opt-in via `--include-partial-messages`. We should **default to off** in production to reduce bus noise; turn on selectively for cost-tracker or live-UI observers.
- **`session_id`** appears in `system:init` and `result` events → we can capture it on init, persist it in the StoryAgent, and use `--resume <session_id>` for sub-turn resumption in Phase 1 multi-turn agents.
- **Mid-flight injection guarantee** means the L2 (sub-turn) architecture proposed in the plan is even more elegant than we thought: we don't need to spawn-new-Claude per sub-turn. One long-lived Claude per story can absorb bus injections naturally between turns, preserving its cohesion.
- **Targeted vs broadcast user messages**: spike used a custom `TargetedUserMessageItem` with explicit `recipientId`. Phase 1 needs a canonical `AgentTargetedMessage` ContextItem (or routing convention) — broadcasting plain `UserMessageItem` to every Claude is wrong in multi-agent envs.

## What we did NOT test (deferred)

- `--resume` to continue an existing session across separate Claude invocations (Phase 1)
- Hook integration (`PreToolUse` / `PostToolUse`) for true preemption — Phase 5
- Behavior on `claude` process kill mid-stream — Phase 1 robustness
- Bus race when many participants emit synchronously inside `onContextItem` — already flagged in main plan risk register (R5)

## Go / no-go for Phase 1

**GO.** Zero blocking findings. The architecture as planned is viable end-to-end.

## Estimate revision

The locked-plan estimate of 3-5 days for Phase 1 stands. Mid-flight injection working out-of-the-box is a small positive signal — fewer workarounds needed.
