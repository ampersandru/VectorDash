# Local decision model (BETA)

VectorDash can optionally use [Qev 0.8B](https://huggingface.co/labqoat/qev-0.8b-GGUF) to select observed controls for computer and browser tools. Your main chat model still plans the task, supplies text, reads results, and answers. Qev is a choice model, not a replacement chat model.

## Setup

In **Models and AI → Computer and browser use**, enable the tools you want, then configure **Local decision model**:

- **Local:** download Qev Q8_0 (about 812 MB), or choose its GGUF. VectorDash reuses the Hardware llama-server executable and GPU selection, starting a separate process when needed. Use the runtime override for a newer compatible llama-server. Test/load checks `/v1/systemone`; Unload releases the decision model.
- **Remote:** enter a decision-server URL, optional API key, and optional model ID. The server must implement the llama.cpp `/v1/systemone` choice API; an ordinary chat-completions endpoint is insufficient. The server receives compact observed window/page information after tool permission is granted. The API key is protected using the existing Windows credential protection.

The existing EmbeddingGemma process serves embeddings and cannot load Qev simultaneously. The executable and GPU can be shared; model memory is additional. The optional download does not replace EmbeddingGemma or install Python. Qev stays loaded until unloaded or VectorDash exits.

## Chat modes

When Computer or Browser is checked, its dropdown appears immediately to its left:

- **Off:** the main chat model makes all control decisions.
- **Suggest:** Qev proposes an observed target; the main model decides whether to execute it.
- **Execute:** the main model may submit a short explicit plan. Qev selects targets and VectorDash runs each action through the usual permission, freshness, protected-target, cancellation, and action-budget checks. A further local step requires an explicit verified postcondition.

Each tool has its own mode. Uncertain decisions, missing controls, failed verification, and unavailable servers return control to the main model. Confidence never grants permission or proves task completion. Existing Allow once / Allow this session / Deny controls and emergency stop still apply.

## Testing

1. Start with Suggest and ask to open Calculator and press a number. Confirm a local decision appears in per-step diagnostics, while the main model remains responsible for acting.
2. Try Execute with simple explicit steps, then a browser form. Check actual visible results, not only the model's answer.
3. Stop tools during a task, deny permission, or use an unavailable decision URL. Confirm later local actions stop or fall back.
4. Compare the same task with Off. Diagnostics report decision calls, executed actions, and decision elapsed time. Overall speed depends on whether the main model provides useful short plans.

Current integration uses structured UIA/DOM observations, not Qev screenshot interpretation. Screen-only controls fall back to the main model. This remains experimental and does not guarantee faster or correct decisions for every application.
