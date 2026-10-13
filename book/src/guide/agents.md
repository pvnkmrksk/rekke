<!-- The MCP guide lives in docs/mcp.md, so the repository and the book share
     one copy. tests/docs.rs checks that it names every tool. -->
{{#include ../../../docs/mcp.md}}

## Tool reference

Every tool's arguments, as the server reports them in `tools/list`, are in
[MCP tool schemas](../reference/mcp-tools.md).

## Notes for coding agents

Agents that change Rekkefly's code, rather than operate rigs, should read
[`AGENTS.md`](https://github.com/pvnkmrksk/rekkefly/blob/main/AGENTS.md): the
checks to run before every commit, where each area of the code lives and which
tests cover it, and the house rules (never regress tracking, never lose user
data).
