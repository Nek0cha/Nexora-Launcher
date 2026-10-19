//! インスタンスの設定の JVM 引数（利用者が入力した文字列）の解釈。

/// 空白で区切る。`"` で囲んだ部分は空白を含めて 1 つの引数にする。
/// 閉じていない `"` は末尾までを 1 つの引数とみなす。引用符そのものは含めない。
/// 空の引数（`""` など）は、java がメインクラスと取り違えて起動できなくなるため、返さない。
pub fn split(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for c in input.chars() {
        if c == '"' {
            quoted = !quoted;
        } else if c.is_whitespace() && !quoted {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// `-Xmx` で始まる引数を除き、除いたかを返す（メモリの設定を優先するため）。
pub fn strip_memory(args: Vec<String>) -> (Vec<String>, bool) {
    let before = args.len();
    let kept: Vec<String> = args
        .into_iter()
        .filter(|a| !a.starts_with("-Xmx"))
        .collect();
    let stripped = kept.len() != before;
    (kept, stripped)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn split_handles_quotes() {
        assert_eq!(
            split(r#"-Dfoo="a b" -Xss1M"#),
            strings(&["-Dfoo=a b", "-Xss1M"])
        );
        assert_eq!(
            split(r#""-Dpath=C:\My Games" -XX:+UseG1GC"#),
            strings(&[r"-Dpath=C:\My Games", "-XX:+UseG1GC"])
        );
    }

    #[test]
    fn split_keeps_unclosed_quote_to_the_end() {
        assert_eq!(
            split(r#"-Xss1M -Dfoo="a b  c"#),
            strings(&["-Xss1M", "-Dfoo=a b  c"])
        );
        // 引用符だけのときも、空の引数を作らない
        assert!(split(r#"""#).is_empty());
    }

    #[test]
    fn split_ignores_extra_whitespace_and_empty_input() {
        assert!(split("").is_empty());
        assert!(split("   \t\n ").is_empty());
        assert_eq!(
            split("  -Xss1M \t  -XX:+UseG1GC\n"),
            strings(&["-Xss1M", "-XX:+UseG1GC"])
        );
        // 空の引数（""）は java がメインクラスと取り違えるため、渡さない
        assert_eq!(split(r#"-Xss1M "" -Da=1"#), strings(&["-Xss1M", "-Da=1"]));
    }

    #[test]
    fn strip_memory_removes_xmx_only() {
        let (args, stripped) = strip_memory(strings(&["-Xmx4G", "-Xms1G", "-XX:+UseG1GC"]));
        assert_eq!(args, strings(&["-Xms1G", "-XX:+UseG1GC"]));
        assert!(stripped);
        let (args, stripped) = strip_memory(strings(&["-Xms1G"]));
        assert_eq!(args, strings(&["-Xms1G"]));
        assert!(!stripped);
    }

    #[test]
    fn xmx_inside_quotes_is_also_stripped() {
        let (args, stripped) = strip_memory(split(r#"-Xss1M "-Xmx8G" -Xmx2G"#));
        assert_eq!(args, strings(&["-Xss1M"]));
        assert!(stripped);
    }
}
