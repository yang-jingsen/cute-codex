pub const SUMMARIZATION_PROMPT: &str = include_str!("../templates/compact/prompt.md");
pub const SUMMARY_PREFIX: &str =
    include_str!("../templates/compact/summary_prefix.md").trim_ascii_end();

#[cfg(test)]
mod tests {
    use super::SUMMARIZATION_PROMPT;
    use super::SUMMARY_PREFIX;

    #[test]
    fn compact_prompts_preserve_the_active_execution_frontier() {
        assert!(
            SUMMARIZATION_PROMPT.contains("internal continuity checkpoint"),
            "the summary prompt must identify compaction as an internal boundary"
        );
        assert!(
            SUMMARIZATION_PROMPT.contains("exact execution frontier and next unblocked action"),
            "the summary prompt must preserve an executable continuation point"
        );
        assert!(
            SUMMARY_PREFIX.contains("the user's active request has already been accepted"),
            "the resumed model must not reinterpret the checkpoint as a new request"
        );
        assert!(
            SUMMARY_PREFIX.contains("immediately continue from the exact execution frontier"),
            "the resumed model must continue without another user prompt"
        );
    }
}
