# ADR-010: Targeted messaging only, no broadcast

**Date:** 2026-03-02
**Status:** Accepted

## Context

Flywheel's experience: agents default to broadcasting everything if you let them, burning context on irrelevant messages. Each message consumes tokens from the recipient's context window. Broadcast messaging at scale means every agent's context fills with noise from every other agent.

## Decision

All messages must address specific recipients. There is no broadcast channel. Messages are stored in SQLite (durable, survives agent death) and delivered to specific agent entity IDs. The messaging API requires a `to` field — there is no "send to all" option.

Messages are stored in the database, not injected into agent context windows directly. Agents query for their messages when they need them, keeping communication off the token budget until explicitly consumed.

## Consequences

- Agent context windows stay focused on their actual work
- Agents must know who to talk to (requires knowing other agents' IDs/roles)
- No easy way to announce system-wide events (must enumerate recipients or use a coordination entity)
- Message delivery is pull-based (agent checks for messages) not push-based
- Scales well — message volume per agent is proportional to their collaborators, not total agent count
