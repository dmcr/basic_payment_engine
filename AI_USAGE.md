# AI Usage

## Declaration

I will be using claude-code for this project.

### General high level flow
At a high level the ai usage flow is;
Foreach `milestone`: Discussion>>Artifacts > Plan>>Implement > Verify>>Review

Discssuion>>Artifacts
A back and forth or an AI led interview to surface gaps in my framing, knowledge gaps 
etc. iterating on relavent artifacts as output e.g. a spec until I am satisfied.

Plan>>Implement
Using the artifacts from the prior step iterate on an implementation plan until I am 
satisfied. Clear the context then use plan as implementation prompt.

Verify>>Review
In parralel myself and ai verify against the plan and artifacts and then finally review.

### Narration
I will be narrating my usage of AI in the [`DEVLOG.md`](./DEVLOG.md) alongside 
engineering narration.

I believe a running narration of usage alongside engineering decisions
to be more insightful than providing a raw dumpt of `prompts`/`sessions`.

### Raw key prompts and context
Artifacts can be considered as key raw context for an initial prompts context.
You can consider plan artifacts themselves as both key and raw prompts for your viewing.




