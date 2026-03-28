/// Format a number with comma separators (e.g. 1234567 -> "1,234,567").
pub fn format_commas(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

/// Calculate estimated USD cost from token counts using Sonnet pricing:
/// $3.00 per million input tokens, $15.00 per million output tokens.
pub fn calculate_cost(input_tokens: u64, output_tokens: u64) -> f64 {
    let input_cost = input_tokens as f64 * 3.0 / 1_000_000.0;
    let output_cost = output_tokens as f64 * 15.0 / 1_000_000.0;
    input_cost + output_cost
}

/// Produce a combined token display string:
/// "Tokens: 12,345 in / 23,456 out (~$0.42)"
pub fn format_token_display(input_tokens: u64, output_tokens: u64) -> String {
    let cost = calculate_cost(input_tokens, output_tokens);
    format!(
        "Tokens: {} in / {} out (~${:.2})",
        format_commas(input_tokens),
        format_commas(output_tokens),
        cost
    )
}
