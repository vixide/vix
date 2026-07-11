# Ai Diff

Reviewable diff for AI text transforms (Annotate / Improve).

Rather than overwrite the buffer the moment the assistant replies, the host
builds a [`Review`] of the proposed change and lets the user accept or reject
it hunk by hunk. Each [`Seg`]ment is either unchanged context or a change the
user can toggle; [`Review::result`] reconstructs the final text from the
accepted choices. Pure data over `split_inclusive('\n')` lines, so the result
is an exact reconstruction (no line-ending guessing) and unit-testable.
