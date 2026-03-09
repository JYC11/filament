# QA-05: Relations & Graph

**Date**: 2026-03-09 (Session 85)
**Environment**: `/tmp/fl-qa-relations`, fresh `fl init`
**Result**: 6/6 PASS, 0 bugs

## Setup

Created 8 entities (3 tasks, 1 module, 1 service, 1 doc, 1 agent, 1 plan) with various relation types forming a multi-depth graph.

## Results

| ID | Test | Result | Notes |
|----|------|--------|-------|
| RG-01 | All relation types | PASS | blocks, depends_on, produces, owns, relates_to, assigned_to — all created and verified in inspect |
| RG-02 | Unrelate | PASS | Removed relates_to; verified gone from both `fl context` and `fl inspect` |
| RG-03 | Context BFS depth 1/2/3 | PASS | Chain Alpha→Beta→Gamma→Theta; depth 1=1 entity, depth 2=3, depth 3=5 — all correct |
| RG-04 | PageRank | PASS | Hub nodes (Gamma, Theta) rank highest; source-only nodes (Alpha, Zeta, Eta) rank lowest — plausible |
| RG-05 | Degree centrality | PASS | All in/out/total counts verified correct against known graph topology |
| RG-06 | Relate nonexistent entity | PASS | Exit code 3, clear error, JSON format includes code/message/hint/retryable |

## Graph Topology Tested

```
Alpha --(blocks)--> Beta --(relates_to)--> Gamma --(depends_on)--> Delta --(produces)--> Epsilon
                      ^                      |
              Zeta --(assigned_to)           +--(relates_to)--> Theta
                                                                  ^
                                                 Eta --(owns)-----+
```
