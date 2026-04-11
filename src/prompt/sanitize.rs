pub fn remove_content_tags(input: &str, tag: &str) -> String {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut output = input.to_owned();

    while let (Some(start), Some(end)) = (output.find(&open), output.find(&close)) {
        if end < start {
            break;
        }
        let close_end = end + close.len();
        output.replace_range(start..close_end, "");
    }

    output.trim().to_owned()
}

pub fn sanitize_model_output(input: &str) -> String {
    let mut output = input.trim().to_owned();
    for tag in ["think", "thinking"] {
        output = remove_content_tags(&output, tag);
    }
    output.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_reasoning_tags() {
        assert_eq!(
            remove_content_tags("<think>hidden</think>\nfeat: add cli", "think"),
            "feat: add cli"
        );
    }

    #[test]
    fn sanitize_model_output_removes_known_reasoning_tags() {
        assert_eq!(
            sanitize_model_output("  <thinking>hidden</thinking>\nfeat: add cli  "),
            "feat: add cli"
        );
    }
}
