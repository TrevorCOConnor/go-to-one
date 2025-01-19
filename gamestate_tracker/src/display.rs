fn buffer(text: &str, width: usize) -> String {
    if text.len() > width {
        panic!("Text is too big to buffer with {}", width);
    } else {
        let needed = width - text.len();
        let buffer = " ".repeat(needed);

        format!("{}{}", text, buffer)
    }
}

pub fn merge_displays(left: Vec<String>, right: Vec<String>) -> String {
    let max_left = left
        .iter()
        .map(|line| line.len())
        .max()
        .expect("Empty display given");

    left.iter()
        .zip(right.iter())
        .map(|(l, r)| format!("{} | {}\n", buffer(l, max_left), r))
        .collect()
}
