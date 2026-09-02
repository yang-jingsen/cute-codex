You are performing a CONTEXT CHECKPOINT COMPACTION. Create a handoff summary for another LLM that will resume the task. This is an internal continuity checkpoint, not a completion or approval boundary. Treat the current user request as already active unless the work is genuinely complete or blocked.

Include:
- Continuation status: active, completed, or blocked
- Current progress and key decisions made
- Important context, constraints, or user preferences
- The exact execution frontier and next unblocked action
- What remains to be done after that action
- Any critical data, examples, or references needed to continue

If work is active and unblocked, make the next action explicit and state that it should be executed immediately. Do not turn compaction itself into a stop condition, ask the user to repeat the request, or require another "continue" message. Be concise, structured, and focused on helping the next LLM seamlessly continue the work.
