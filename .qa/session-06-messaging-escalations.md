# QA-06: Messaging & Escalations

**Date**: 2026-03-09 (Session 85)
**Environment**: `/tmp/fl-qa-relations` (reused from QA-05)
**Result**: 4/4 PASS, 0 bugs

## Setup

Created 2 agents (Agent-A, Agent-B) and 1 "user" agent for escalation testing.

## Results

| ID | Test | Result | Notes |
|----|------|--------|-------|
| ME-01 | Send all message types | PASS | text, question, blocker, artifact — all sent successfully |
| ME-02 | Inbox filtering | PASS | Inbox shows correct messages per agent; `message read` removes from inbox |
| ME-03 | Escalation creation | PASS | Blocker/question to "user" appear in `fl escalations` |
| ME-04 | Escalation resolution | PASS | Reading messages clears escalations; "No pending escalations" after all read |

## Observations

- `fl escalations` shows ALL unread blocker/question messages regardless of recipient (not just those addressed to "user"). This is arguably useful — human operator sees everything needing attention — but differs from what the docs imply (send to "user" to create escalations). Not filing as bug since behavior is reasonable.
