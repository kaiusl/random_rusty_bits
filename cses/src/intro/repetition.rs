fn solve(input: &str) -> usize {
    if input.is_empty() {
        return 0;
    }

    let mut max = 1;
    let mut chars = input.chars();
    let mut current_len = 1;
    let mut current_char = chars.next().unwrap();

    loop {
        match chars.next() {
            Some(ch) => {
                if ch == current_char {
                    current_len += 1;
                } else {
                    max = max.max(current_len);
                    current_len = 1;
                    current_char = ch;
                }
            }
            None => {
                max = max.max(current_len);
                break;
            }
        }
    }

    max
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        assert_eq!(solve(""), 0);
        assert_eq!(solve("ATTCGGGA"), 3);
        assert_eq!(solve("ATTCCCCCGGGA"), 5);
        assert_eq!(solve("ATCGAGT"), 1);
    }
}
