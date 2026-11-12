pub fn greeting() -> &'static str {
    "hello"
}

#[cfg(test)]
mod tests {
    use super::greeting;

    #[test]
    fn says_hello() {
        assert_eq!(greeting(), "hello");
    }
}
